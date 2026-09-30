//! Append-only snapshot operations and a read-only, clock-independent planner.
use crate::{
    GitHubChange, GitHubClient, GitHubError, RemoteStore, WebDavError, validate_relative_path,
};
use chronicle_core::Snapshot;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const OPERATIONS: &str = "sync-v2/operations/";

fn validate_object_path(path: &str) -> Result<(), String> {
    validate_relative_path(path).map_err(|error| error.to_string())?;
    if std::path::Path::new(path)
        .extension()
        .is_none_or(|ext| ext != "7z")
        || !(path.starts_with("archives/")
            || path.starts_with("sync-v2/objects/")
            || path.starts_with("sync-v2/recovery/"))
    {
        return Err("快照对象路径不属于快照存储区".into());
    }
    Ok(())
}

fn validate_event(event: &Event) -> Result<(), String> {
    operation_path(&event.operation_id)?;
    match &event.kind {
        EventKind::Published { object_path, .. } => validate_object_path(object_path)?,
        EventKind::Deleted {
            object_path,
            related_revisions,
            observed_revisions,
            ..
        } => {
            validate_object_path(object_path)?;
            if object_path != &format!("sync-v2/recovery/{}.7z", event.operation_id) {
                return Err("回收路径与删除记录不一致".into());
            }
            for revision in related_revisions {
                operation_path(&revision.operation_id)?;
                if !observed_revisions.contains(&revision.operation_id)
                    || !matches!(&revision.kind, EventKind::Revised { snapshot, .. } if event.snapshot().is_some_and(|deleted| snapshot.id == deleted.id && snapshot.entry_id == deleted.entry_id))
                {
                    return Err("删除的修订依据格式无效".into());
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod record_validation_tests {
    use super::*;
    #[test]
    fn snapshot_objects_cannot_target_protocol_or_library_configuration() {
        assert!(validate_object_path("catalog.json").is_err());
        assert!(validate_object_path("sync-v2/operations/record.7z").is_err());
        assert!(validate_object_path("archives/game/../../library.7z").is_err());
        assert!(validate_object_path("archives/game/file.7z").is_ok());
        assert!(validate_object_path("sync-v2/objects/file.7z").is_ok());
        assert!(validate_object_path("sync-v2/recovery/file.7z").is_ok());
    }
}

/// Shared protocol I/O; every backend uses the same planner and immutable log.
pub enum SnapshotRemote {
    Store(RemoteStore),
    GitHub(GitHubClient),
}
impl SnapshotRemote {
    /// Reads an optional protocol object.
    ///
    /// # Errors
    /// Returns an error for an invalid path or a remote read failure other than not found.
    pub async fn optional_bytes(&self, path: &str) -> Result<Option<Vec<u8>>, String> {
        validate_relative_path(path).map_err(|error| error.to_string())?;
        match self {
            Self::Store(store) => match store.get_bytes(path).await {
                Ok(bytes) => Ok(Some(bytes)),
                Err(WebDavError::NotFound(_)) => Ok(None),
                Err(error) => Err(error.to_string()),
            },
            Self::GitHub(client) => match client.download_file(path).await {
                Ok(bytes) => Ok(Some(bytes)),
                Err(GitHubError::NotFound(_)) => Ok(None),
                Err(error) => Err(error.to_string()),
            },
        }
    }
    /// Writes a protocol object after ensuring its parent collection exists.
    ///
    /// # Errors
    /// Returns an error for an invalid path or failed remote write.
    pub async fn put(&self, path: &str, bytes: Vec<u8>) -> Result<(), String> {
        validate_relative_path(path).map_err(|error| error.to_string())?;
        match self {
            Self::Store(store) => {
                if let Some((parent, _)) = path.rsplit_once('/') {
                    store
                        .ensure_collection(parent)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                store
                    .put_atomic(path, bytes)
                    .await
                    .map_err(|error| error.to_string())
            }
            Self::GitHub(client) => client
                .commit_changes(
                    "Chronicle snapshot protocol",
                    vec![GitHubChange {
                        path: path.into(),
                        contents: Some(bytes),
                    }],
                )
                .await
                .map_err(|error| error.to_string()),
        }
    }
    /// Removes a protocol object.
    ///
    /// # Errors
    /// Returns an error for an invalid path or failed remote deletion.
    pub async fn delete(&self, path: &str) -> Result<(), String> {
        validate_relative_path(path).map_err(|error| error.to_string())?;
        match self {
            Self::Store(store) => store.delete(path).await.map_err(|error| error.to_string()),
            Self::GitHub(client) => client
                .commit_changes(
                    "Chronicle snapshot purge",
                    vec![GitHubChange {
                        path: path.into(),
                        contents: None,
                    }],
                )
                .await
                .map_err(|error| error.to_string()),
        }
    }
    /// Reads and validates append-only operation records.
    ///
    /// # Errors
    /// Returns an error if listing, reading, parsing, or validation fails.
    pub async fn load_events(&self) -> Result<Vec<Event>, String> {
        let files = match self {
            Self::Store(store) => match store.list_json_files(OPERATIONS).await {
                Ok(files) => files,
                Err(WebDavError::NotFound(_)) => Vec::new(),
                Err(error) => return Err(error.to_string()),
            },
            Self::GitHub(client) => client
                .list_json_files(OPERATIONS)
                .await
                .map_err(|error| error.to_string())?,
        };
        let mut events = Vec::new();
        for file in files {
            let bytes = self
                .optional_bytes(&file)
                .await?
                .ok_or_else(|| "操作记录在列举后丢失，请重新预览".to_owned())?;
            let event: Event = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            if operation_path(&event.operation_id)? != file {
                return Err("操作记录标识与文件名不一致".into());
            }
            validate_event(&event)?;
            for revision in event.related_revisions() {
                events.push(revision.clone());
            }
            events.push(event);
        }
        Ok(events)
    }
    /// Publishes an immutable operation record and verifies the result.
    ///
    /// # Errors
    /// Returns an error for a conflicting ID, invalid record, or failed remote I/O.
    pub async fn append_event(&self, event: &Event) -> Result<(), String> {
        validate_event(event)?;
        let path = operation_path(&event.operation_id)?;
        if let Some(bytes) = self.optional_bytes(&path).await? {
            let existing: Event =
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            return if existing == *event {
                Ok(())
            } else {
                Err("操作 ID 已存在且内容不同，禁止覆盖".into())
            };
        }
        self.put(
            &path,
            serde_json::to_vec(event).map_err(|error| error.to_string())?,
        )
        .await?;
        let verified = self
            .optional_bytes(&path)
            .await?
            .ok_or_else(|| "发布操作记录后无法读取".to_owned())?;
        let verified: Event =
            serde_json::from_slice(&verified).map_err(|error| error.to_string())?;
        if verified != *event {
            return Err("发布操作记录校验失败".into());
        }
        Ok(())
    }
}
fn operation_path(id: &str) -> Result<String, String> {
    if id.is_empty()
        || id.len() > 200
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("操作 ID 无效".into());
    }
    Ok(format!("{OPERATIONS}{id}.json"))
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownDevice {
    pub id: String,
    pub name: String,
    pub revision: u64,
    pub last_successful_sync_at: Option<u64>,
}
#[must_use]
pub fn known_devices(events: &[Event]) -> Vec<KnownDevice> {
    let mut devices: BTreeMap<String, KnownDevice> = BTreeMap::new();
    for event in events {
        if event.device.id.is_empty() {
            continue;
        }
        let device = devices
            .entry(event.device.id.clone())
            .or_insert_with(|| KnownDevice {
                id: event.device.id.clone(),
                name: event.device.name.clone(),
                revision: event.device.revision,
                last_successful_sync_at: None,
            });
        if event.device.revision > device.revision
            || matches!(event.kind, EventKind::DeviceNamed)
                && event.device.revision == device.revision
        {
            device.name.clone_from(&event.device.name);
            device.revision = event.device.revision;
        }
        if let EventKind::DeviceSynced { at_ms } = event.kind {
            device.last_successful_sync_at =
                Some(device.last_successful_sync_at.unwrap_or(0).max(at_ms));
        }
    }
    devices.into_values().collect()
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub operation_id: String,
    pub device: Device,
    pub kind: EventKind,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventKind {
    Published {
        snapshot: Snapshot,
        object_path: String,
        #[serde(default)]
        entry: serde_json::Value,
    },
    Revised {
        snapshot: Snapshot,
        parents: Vec<String>,
    },
    Deleted {
        snapshot: Snapshot,
        reason: String,
        observed_revisions: Vec<String>,
        object_path: String,
        #[serde(default)]
        deleted_at_ms: u64,
        #[serde(default)]
        related_revisions: Vec<Event>,
        #[serde(default)]
        entry: serde_json::Value,
    },
    DeviceNamed,
    DeviceSynced {
        at_ms: u64,
    },
    Purged {
        snapshot_id: String,
        deletion_id: String,
    },
}
impl Event {
    fn related_revisions(&self) -> &[Event] {
        match &self.kind {
            EventKind::Deleted {
                related_revisions, ..
            } => related_revisions,
            _ => &[],
        }
    }
    #[must_use]
    pub fn snapshot(&self) -> Option<&Snapshot> {
        match &self.kind {
            EventKind::Published { snapshot, .. }
            | EventKind::Revised { snapshot, .. }
            | EventKind::Deleted { snapshot, .. } => Some(snapshot),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Upload,
    Download,
    UploadRevision,
    DownloadRevision,
    RecycleLocal,
    RecycleRemote,
    Conflict,
    Unchanged,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedSnapshot {
    pub snapshot: Snapshot,
    pub action: Action,
    pub selected: bool,
    pub reason: Option<String>,
    pub deletion: Option<Event>,
    pub revision: Option<Event>,
    pub local_snapshot: Option<Snapshot>,
}

/// Plans only explicit operations. Absence from a catalog never means deletion.
///
/// # Panics
/// Panics only if an internally grouped node has no local, remote, or deletion snapshot.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn plan_sync(
    local: &[Snapshot],
    remote: &[Event],
    pending: &[Event],
    upload_only: bool,
) -> Vec<PlannedSnapshot> {
    let mut by_id: BTreeMap<String, Vec<&Event>> = BTreeMap::new();
    let mut seen: BTreeMap<&str, &Event> = BTreeMap::new();
    let mut invalid_ids = BTreeSet::new();
    for event in remote
        .iter()
        .chain(pending)
        .flat_map(|event| std::iter::once(event).chain(event.related_revisions()))
    {
        if let Some(existing) = seen.get(event.operation_id.as_str()) {
            if *existing != event {
                if let Some(snapshot) = event.snapshot() {
                    invalid_ids.insert(snapshot.id.clone());
                }
                if let Some(snapshot) = existing.snapshot() {
                    invalid_ids.insert(snapshot.id.clone());
                }
            }
        } else {
            seen.insert(&event.operation_id, event);
            if let Some(snapshot) = event.snapshot() {
                by_id.entry(snapshot.id.clone()).or_default().push(event);
            }
        }
    }
    let local: BTreeMap<_, _> = local
        .iter()
        .map(|snapshot| (snapshot.id.clone(), snapshot))
        .collect();
    let ids: BTreeSet<_> = local.keys().chain(by_id.keys()).cloned().collect();
    let mut plan = Vec::new();
    for id in ids {
        let records = by_id.get(&id).cloned().unwrap_or_default();
        let published = records.iter().find_map(|event| match &event.kind {
            EventKind::Published { snapshot, .. } => Some(snapshot),
            _ => None,
        });
        let revisions = records
            .iter()
            .filter(|event| matches!(event.kind, EventKind::Revised { .. }))
            .copied()
            .collect::<Vec<_>>();
        let superseded: BTreeSet<_> = revisions
            .iter()
            .flat_map(|event| match &event.kind {
                EventKind::Revised { parents, .. } => parents.iter(),
                _ => unreachable!(),
            })
            .collect();
        let frontier = revisions
            .iter()
            .filter(|event| !superseded.contains(&event.operation_id))
            .collect::<Vec<_>>();
        let current = frontier
            .first()
            .and_then(|event| event.snapshot())
            .or(published);
        let local_snapshot = local.get(&id).copied();
        let deletion = records
            .iter()
            .find(|event| matches!(event.kind, EventKind::Deleted { .. }))
            .copied();
        let snapshot = local_snapshot
            .or(current)
            .or_else(|| deletion.and_then(Event::snapshot))
            .or_else(|| records.first().and_then(|event| event.snapshot()))
            .unwrap()
            .clone();
        let incompatible_publications = records
            .iter()
            .filter_map(|event| match &event.kind {
                EventKind::Published { snapshot, .. } => Some(snapshot),
                _ => None,
            })
            .any(|snapshot| Some(snapshot) != published);
        let invalid_revisions = !revisions.is_empty() && frontier.is_empty()
            || published.is_some_and(|base| {
                revisions.iter().any(|event| {
                    let mut expected = base.clone();
                    let revised = event.snapshot().unwrap();
                    expected.note.clone_from(&revised.note);
                    expected.locked = revised.locked;
                    expected.metadata_updated_at_ms = revised.metadata_updated_at_ms;
                    expected != *revised
                })
            });
        let (action, reason) =
            if invalid_ids.contains(&id) || incompatible_publications || invalid_revisions {
                if upload_only {
                    continue;
                }
                (
                    Action::Conflict,
                    Some("同一节点存在不一致的发布记录，保留内容".into()),
                )
            } else if let Some(deletion) = deletion {
                if upload_only {
                    continue;
                }
                let EventKind::Deleted {
                    snapshot: deleted,
                    observed_revisions,
                    ..
                } = &deletion.kind
                else {
                    unreachable!()
                };
                let causally_older = local_snapshot.is_some_and(|local| {
                    published == Some(local)
                        || revisions.iter().any(|event| {
                            observed_revisions.contains(&event.operation_id)
                                && event.snapshot() == Some(local)
                        })
                });
                let conflict = local_snapshot
                    .is_some_and(|snapshot| snapshot != deleted && !causally_older)
                    || current.is_some_and(|snapshot| snapshot.locked || snapshot != deleted)
                    || observed_revisions
                        .iter()
                        .any(|id| !revisions.iter().any(|event| &event.operation_id == id))
                    || revisions
                        .iter()
                        .any(|event| !observed_revisions.contains(&event.operation_id));
                if conflict {
                    (
                        Action::Conflict,
                        Some("删除与锁定或元数据修改冲突，保留内容".into()),
                    )
                } else if !remote
                    .iter()
                    .any(|event| event.operation_id == deletion.operation_id)
                {
                    (Action::RecycleRemote, None)
                } else if local_snapshot.is_some() {
                    (Action::RecycleLocal, None)
                } else {
                    (Action::Unchanged, None)
                }
            } else if published.is_none() && local_snapshot.is_some() {
                (Action::Upload, None)
            } else if frontier.len() == 1
                && local_snapshot.is_some()
                && local_snapshot == current
                && !remote
                    .iter()
                    .any(|event| event.operation_id == frontier[0].operation_id)
            {
                if upload_only {
                    continue;
                }
                (Action::UploadRevision, None)
            } else if frontier.len() == 1
                && local_snapshot.is_some()
                && local_snapshot != current
                && records.iter().any(|event| {
                    !matches!(event.kind, EventKind::Deleted { .. })
                        && event.snapshot() == local_snapshot
                })
            {
                if upload_only {
                    continue;
                }
                (Action::DownloadRevision, None)
            } else if frontier.len() > 1 || local_snapshot.zip(current).is_some_and(|(a, b)| a != b)
            {
                if upload_only {
                    continue;
                }
                (
                    Action::Conflict,
                    Some("快照元数据存在并发修改，需重新核对".into()),
                )
            } else {
                match (local_snapshot, current) {
                    (Some(_), None) => (Action::Upload, None),
                    (None, Some(_)) if !upload_only => (Action::Download, None),
                    (Some(_), Some(_)) if !upload_only => (Action::Unchanged, None),
                    _ => continue,
                }
            };
        let snapshot = if action == Action::RecycleLocal {
            deletion.unwrap().snapshot().unwrap().clone()
        } else if action == Action::DownloadRevision {
            current.unwrap().clone()
        } else {
            snapshot
        };
        plan.push(PlannedSnapshot {
            snapshot,
            action,
            selected: matches!(
                action,
                Action::Upload
                    | Action::Download
                    | Action::UploadRevision
                    | Action::DownloadRevision
            ),
            reason,
            deletion: deletion.cloned(),
            revision: frontier.first().map(|event| (**event).clone()),
            local_snapshot: local_snapshot.cloned(),
        });
    }
    plan
}
