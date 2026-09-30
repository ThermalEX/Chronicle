use crate::{AppState, cloud};
use chronicle_core::Snapshot;
use chronicle_storage::{LocalRepository, SnapshotDeletion};
use chronicle_sync::snapshot_protocol::{
    Action, Device, Event, EventKind, PlannedSnapshot, SnapshotRemote, known_devices, plan_sync,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOperation {
    id: String,
    #[serde(flatten)]
    item: PlannedSnapshot,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePlan {
    source_id: String,
    source_name: String,
    upgrade_required: bool,
    error: Option<String>,
    token: String,
    operations: Vec<SyncOperation>,
    archive_names: HashMap<String, String>,
    devices: Vec<chronicle_sync::snapshot_protocol::KnownDevice>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    id: String,
    sources: Vec<SourcePlan>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    operation_id: String,
    source_id: String,
    status: String,
    error: Option<String>,
}

static PLANS: OnceLock<Mutex<HashMap<String, SyncPlan>>> = OnceLock::new();
static TASKS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
static WRITERS: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
struct SourceWriteGuard(String);
impl SourceWriteGuard {
    fn acquire(source_id: &str) -> Result<Self, String> {
        if !WRITERS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| "同步任务状态不可用")?
            .insert(source_id.into())
        {
            return Err("此同步源已有任务正在写入，请稍后重试".into());
        }
        Ok(Self(source_id.into()))
    }
}
impl Drop for SourceWriteGuard {
    fn drop(&mut self) {
        if let Ok(mut writers) = WRITERS.get_or_init(Mutex::default).lock() {
            writers.remove(&self.0);
        }
    }
}
fn plans() -> &'static Mutex<HashMap<String, SyncPlan>> {
    PLANS.get_or_init(Mutex::default)
}
fn tasks() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    TASKS.get_or_init(Mutex::default)
}
fn cancelled(id: &str) -> Result<(), String> {
    if tasks()
        .lock()
        .map_err(|_| "同步任务状态不可用")?
        .get(id)
        .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
    {
        Err("已取消；已提交的操作不会撤销".into())
    } else {
        Ok(())
    }
}
fn begin(id: &str) -> Result<(), String> {
    let mut tasks = tasks().lock().map_err(|_| "同步任务状态不可用")?;
    if tasks.contains_key(id) {
        return Err("同步任务已运行".into());
    }
    tasks.insert(id.into(), Arc::new(AtomicBool::new(false)));
    Ok(())
}
fn end(id: &str) {
    if let Ok(mut tasks) = tasks().lock() {
        tasks.remove(id);
    }
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn validation_token(catalog: &[u8], deletes: &[u8], remote: &[u8], source: &[u8]) -> String {
    let mut hash = Sha256::new();
    for part in [catalog, deletes, remote, source] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn validate_selection(allowed: &[(String, bool)], selected: &[String]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for id in selected {
        if !seen.insert(id)
            || !allowed
                .iter()
                .any(|(candidate, executable)| candidate == id && *executable)
        {
            return Err("选择包含预览以外或不可执行的操作".into());
        }
    }
    Ok(())
}
fn device(repository: &LocalRepository) -> Result<Device, String> {
    let identity = repository
        .read_device()
        .map_err(|error| error.to_string())?;
    Ok(Device {
        id: identity.id,
        name: identity.name,
        revision: identity.revision,
    })
}
fn serialize<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|error| error.to_string())
}
fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|error| error.to_string())
}
fn catalog(root: &Path) -> Result<Value, String> {
    parse(&fs::read(root.join("catalog.json")).map_err(|error| error.to_string())?)
}
fn write(root: &Path, path: &Path, value: &impl Serialize) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("无效状态路径")?).map_err(|error| error.to_string())?;
    let temp = root
        .join(".tmp")
        .join(format!("sync-{}.json", Uuid::new_v4()));
    fs::write(&temp, serialize(value)?).map_err(|error| error.to_string())?;
    fs::rename(temp, path).map_err(|error| error.to_string())
}
fn source_state(root: &Path, source_id: &str) -> PathBuf {
    root.join("config/sync-v2/enabled")
        .join(format!("{}.json", digest(source_id.as_bytes())))
}
pub(crate) fn enabled(root: &Path, source_id: &str) -> bool {
    source_state(root, source_id).exists()
}
fn require_protocol_ready(
    root: &Path,
    source_id: &str,
    upgrade_required: bool,
) -> Result<(), String> {
    if upgrade_required || !enabled(root, source_id) {
        return Err("自动上传前须在同步预览中确认同库全部设备升级并启用新协议".into());
    }
    Ok(())
}

pub(crate) async fn guard_legacy_write(
    state: &State<'_, AppState>,
    source_id: &str,
) -> Result<(), String> {
    let (remote, root, _, _) = cloud::snapshot_remote(state, source_id)?;
    let has_deletes = !state
        .repository
        .lock()
        .map_err(|_| "资料库状态不可用")?
        .list_snapshot_deletions()
        .map_err(|error| error.to_string())?
        .is_empty();
    if enabled(&root, source_id)
        || has_deletes
        || remote
            .optional_bytes("sync-v2/protocol.json")
            .await?
            .is_some()
    {
        return Err("已启用快照删除保护，覆盖操作不可绕过同步预览".into());
    }
    Ok(())
}

fn baseline(catalog: &Value) -> Result<Vec<Event>, String> {
    let mut events = Vec::new();
    for entry in catalog
        .get("entries")
        .and_then(Value::as_array)
        .ok_or("云端目录格式无效")?
    {
        let folder = entry
            .get("folder")
            .and_then(Value::as_str)
            .ok_or("存档路径缺失")?;
        for value in entry
            .get("snapshots")
            .and_then(Value::as_array)
            .ok_or("存档时间线格式无效")?
        {
            let snapshot: Snapshot =
                serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
            let object_path = format!("archives/{folder}/{}", snapshot.archive_name);
            chronicle_sync::validate_relative_path(&object_path)
                .map_err(|error| error.to_string())?;
            let mut event = Event {
                operation_id: String::new(),
                device: Device {
                    id: snapshot.device_id.clone(),
                    name: snapshot.device_name.clone(),
                    revision: 0,
                },
                kind: EventKind::Published {
                    snapshot,
                    object_path,
                    entry: entry.clone(),
                },
            };
            event.operation_id = format!("publish-{}", digest(&serialize(&event)?));
            events.push(event);
        }
    }
    Ok(events)
}
async fn remote_events(remote: &SnapshotRemote) -> Result<(Vec<Event>, bool), String> {
    let marker = remote.optional_bytes("sync-v2/protocol.json").await?;
    if let Some(marker) = &marker {
        let marker: Value = parse(marker)?;
        if marker.get("version").and_then(Value::as_u64) != Some(2) {
            return Err("同步协议版本不支持".into());
        }
    }
    let mut events = remote.load_events().await?;
    if marker.is_some() && events.is_empty() {
        return Err("同步操作记录缺失，不能将此源视为没有变化".into());
    }
    if marker.is_none() {
        if let Some(bytes) = remote.optional_bytes("catalog.json").await? {
            for event in baseline(&parse(&bytes)?)? {
                // Materialized indexes do not carry the v2 object path. Only adopt
                // legacy nodes that have no canonical publication yet.
                if !events.iter().any(|known| matches!((&known.kind, &event.kind), (EventKind::Published { snapshot: known, .. }, EventKind::Published { snapshot, .. }) if known.id == snapshot.id)) { events.push(event); }
            }
        }
    }
    events.sort_by(|left, right| left.operation_id.cmp(&right.operation_id));
    Ok((events, marker.is_none()))
}
fn pending(repository: &LocalRepository) -> Result<Vec<Event>, String> {
    let revisions: Vec<Event> = repository
        .snapshot_revision_records()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|value| serde_json::from_value(value).map_err(|error| error.to_string()))
        .collect::<Result<_, _>>()?;
    let mut dependencies = revisions.clone();
    let known_path = repository
        .root()
        .join("config/snapshot-known-revisions.json");
    if known_path.exists() {
        dependencies.extend(parse::<Vec<Event>>(
            &fs::read(known_path).map_err(|error| error.to_string())?,
        )?);
    }
    let mut events = repository
        .list_snapshot_deletions()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|deletion| Event {
            operation_id: deletion.operation_id.clone(),
            device: Device {
                id: deletion.device.id,
                name: deletion.device.name,
                revision: deletion.device.revision,
            },
            kind: EventKind::Deleted {
                related_revisions: dependencies
                    .iter()
                    .filter(|revision| deletion.observed_revisions.contains(&revision.operation_id))
                    .cloned()
                    .collect(),
                entry: deletion.entry_metadata.unwrap_or(Value::Null),
                snapshot: deletion.snapshot,
                reason: deletion.reason,
                observed_revisions: deletion.observed_revisions,
                deleted_at_ms: deletion.deleted_at_ms,
                object_path: format!("sync-v2/recovery/{}.7z", deletion.operation_id),
            },
        })
        .collect::<Vec<_>>();
    events.extend(revisions);
    Ok(events)
}
fn source_token(
    root: &Path,
    repository: &LocalRepository,
    events: &[Event],
    signature: &[u8],
) -> Result<String, String> {
    if repository.root() != root {
        return Err("资料库位置已改变，请重新预览".into());
    }
    let identity = serialize(&(
        signature,
        root.to_string_lossy(),
        repository
            .read_device()
            .map_err(|error| error.to_string())?,
    ))?;
    Ok(validation_token(
        &fs::read(root.join("catalog.json")).map_err(|error| error.to_string())?,
        &serialize(
            &repository
                .list_snapshot_deletions()
                .map_err(|error| error.to_string())?,
        )?,
        &serialize(events)?,
        &identity,
    ))
}

#[tauri::command(async)]
pub fn list_snapshot_recovery(state: State<'_, AppState>) -> Result<Vec<SnapshotDeletion>, String> {
    state
        .repository
        .lock()
        .map_err(|_| "资料库状态不可用")?
        .list_snapshot_deletions()
        .map_err(|error| error.to_string())
}
#[tauri::command(async)]
pub fn restore_snapshot_recovery(
    state: State<'_, AppState>,
    operation_id: String,
) -> Result<(), String> {
    state
        .repository
        .lock()
        .map_err(|_| "资料库状态不可用")?
        .restore_recycled_snapshot(&operation_id)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn deletion_record(event: &Event) -> Result<SnapshotDeletion, String> {
    let EventKind::Deleted {
        snapshot,
        reason,
        observed_revisions,
        deleted_at_ms,
        ..
    } = &event.kind
    else {
        return Err("此记录不是删除操作".into());
    };
    Ok(SnapshotDeletion {
        operation_id: event.operation_id.clone(),
        snapshot: snapshot.clone(),
        device: chronicle_storage::DeviceIdentity {
            id: event.device.id.clone(),
            name: event.device.name.clone(),
            revision: event.device.revision,
        },
        reason: reason.clone(),
        deleted_at_ms: *deleted_at_ms,
        committed: true,
        purged: false,
        restored_snapshot: None,
        restore_committed: false,
        observed_revisions: observed_revisions.clone(),
        entry_metadata: match &event.kind {
            EventKind::Deleted { entry, .. } if !entry.is_null() => Some(entry.clone()),
            _ => None,
        },
    })
}

#[tauri::command(async)]
pub async fn read_remote_snapshot_recovery(
    state: State<'_, AppState>,
    source_ids: Vec<String>,
) -> Result<Vec<Value>, String> {
    let mut sources = Vec::new();
    for source_id in source_ids {
        let outcome = async {
            let (remote, _, name, _) = cloud::snapshot_remote(&state, &source_id)?;
            let (events, _) = remote_events(&remote).await?;
            let conflicts = plan_sync(&[], &events, &[], false);
            let mut records = Vec::new();
            for event in &events {
                if matches!(event.kind, EventKind::Deleted { .. }) {
                    let mut record = deletion_record(event)?;
                    record.purged = events.iter().any(|item| matches!(&item.kind, EventKind::Purged { deletion_id, .. } if *deletion_id == event.operation_id));
                    records.push(json!({"record":record,"conflict":conflicts.iter().any(|item| item.snapshot.id == record.snapshot.id && item.action == Action::Conflict)}));
                }
            }
            Ok::<_, String>(json!({"sourceId":source_id,"sourceName":name,"records":records,"error":null}))
        }.await;
        sources.push(outcome.unwrap_or_else(
            |error| json!({"sourceId":source_id,"sourceName":source_id,"records":[],"error":error}),
        ));
    }
    Ok(sources)
}

async fn restore_remote(
    repo: &Arc<Mutex<LocalRepository>>,
    remote: &SnapshotRemote,
    operation_id: &str,
) -> Result<(), String> {
    let (events, _) = remote_events(remote).await?;
    let event = events
        .iter()
        .find(|event| event.operation_id == operation_id)
        .ok_or("删除记录不存在")?;
    let mut deletion = deletion_record(event)?;
    if events.iter().any(|item| matches!(&item.kind, EventKind::Purged { deletion_id, .. } if deletion_id == operation_id)) { return Err("此源的回收文件已永久清理".into()); }
    if plan_sync(&[], &events, &[], false)
        .iter()
        .any(|item| item.snapshot.id == deletion.snapshot.id && item.action == Action::Conflict)
    {
        return Err("删除存在锁定或修订冲突，不能恢复".into());
    }
    let EventKind::Deleted { object_path, .. } = &event.kind else {
        unreachable!()
    };
    let bytes = remote
        .optional_bytes(object_path)
        .await?
        .ok_or("云端回收文件缺失")?;
    if digest(&bytes) != deletion.snapshot.object_hash {
        return Err("回收文件校验失败".into());
    }
    if serialize(&remote_events(remote).await?.0)? != serialize(&events)? {
        return Err("云端状态改变，请重新读取回收区".into());
    }
    let repo = repo.lock().map_err(|_| "资料库状态不可用")?;
    if deletion.entry_metadata.is_none() {
        deletion.entry_metadata = events.iter().find_map(|event| match &event.kind {
            EventKind::Published {
                snapshot, entry, ..
            } if snapshot.id == deletion.snapshot.id => Some(entry.clone()),
            _ => None,
        });
    }
    repo.accept_snapshot_deletion(deletion, &bytes)
        .map_err(|error| error.to_string())?;
    repo.restore_recycled_snapshot_from_cloud(operation_id, &bytes)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub async fn restore_remote_snapshot_recovery(
    state: State<'_, AppState>,
    source_id: String,
    operation_id: String,
) -> Result<(), String> {
    let _writer = SourceWriteGuard::acquire(&source_id)?;
    let (remote, _, _, _) = cloud::snapshot_remote(&state, &source_id)?;
    restore_remote(&state.repository, &remote, &operation_id).await
}

#[tauri::command(async)]
pub async fn purge_snapshot_recovery(
    state: State<'_, AppState>,
    operation_id: String,
    source_ids: Vec<String>,
    purge_local: bool,
    confirmed: bool,
) -> Result<Vec<OperationResult>, String> {
    if !confirmed {
        return Err("永久清理须再次确认".into());
    }
    let identity = {
        let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
        device(&repository)?
    };
    let mut results = Vec::new();
    for source_id in source_ids {
        let outcome = async {
            let _writer = SourceWriteGuard::acquire(&source_id)?;
            let (remote, _, _, _) = cloud::snapshot_remote(&state, &source_id)?;
            let (events, _) = remote_events(&remote).await?;
            let event = events.iter().find(|event| event.operation_id == operation_id).ok_or("此源尚未确认删除，不能清理")?;
            let EventKind::Deleted { snapshot, object_path, .. } = &event.kind else { return Err("此记录不是删除操作".into()); };
            if plan_sync(&[], &events, &[], false).iter().any(|op| op.snapshot.id == snapshot.id && op.action == Action::Conflict) { return Err("节点存在锁定或元数据冲突，保留文件".into()); }
            let mut paths = vec![object_path.clone()];
            for event in &events {
                if let EventKind::Published { snapshot: item, object_path, .. } = &event.kind { if item.id == snapshot.id { paths.push(object_path.clone()); } }
            }
            paths.sort(); paths.dedup();
            for path in &paths {
                if events.iter().any(|event| matches!(&event.kind, EventKind::Published { snapshot: other, object_path, .. } if other.id != snapshot.id && object_path == path)) { return Err("文件由其他节点共享，不能永久删除".into()); }
                if let Some(bytes) = remote.optional_bytes(path).await? { if digest(&bytes) != snapshot.object_hash { return Err("待清理文件校验失败，保留内容".into()); } }
            }
            if serialize(&remote_events(&remote).await?.0)? != serialize(&events)? { return Err("清理前云端状态改变，请重试".into()); }
            let purged = Event { operation_id: format!("purge-{operation_id}"), device: identity.clone(), kind: EventKind::Purged { snapshot_id: snapshot.id.clone(), deletion_id: operation_id.clone() } };
            if !events.iter().any(|event| event.operation_id == purged.operation_id && matches!(&event.kind, EventKind::Purged { snapshot_id, deletion_id } if snapshot_id == &snapshot.id && deletion_id == &operation_id)) { remote.append_event(&purged).await?; }
            for path in paths { if remote.optional_bytes(&path).await?.is_some() { remote.delete(&path).await?; } }
            Ok::<_, String>(())
        }.await;
        results.push(OperationResult {
            operation_id: operation_id.clone(),
            source_id,
            status: if outcome.is_ok() { "success" } else { "failed" }.into(),
            error: outcome.err(),
        });
    }
    if purge_local {
        let outcome = state
            .repository
            .lock()
            .map_err(|_| "资料库状态不可用")?
            .purge_recycled_snapshot(&operation_id)
            .map_err(|error| error.to_string());
        results.push(OperationResult {
            operation_id,
            source_id: "local".into(),
            status: if outcome.is_ok() { "success" } else { "failed" }.into(),
            error: outcome.err(),
        });
    }
    Ok(results)
}

#[tauri::command(async)]
pub async fn preview_snapshot_sync(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    source_ids: Vec<String>,
    entry_ids: Vec<String>,
    snapshot_id: Option<String>,
) -> Result<SyncPlan, String> {
    begin(&request_id)?;
    let mut plan = SyncPlan {
        id: request_id.clone(),
        sources: Vec::new(),
    };
    for source_id in source_ids {
        if cancelled(&request_id).is_err() {
            end(&request_id);
            return Err("预览已取消".into());
        }
        let result = async {
            let (remote, root, name, signature) = cloud::snapshot_remote(&state, &source_id)?;
            let (events, remote_upgrade) = remote_events(&remote).await?;
            cancelled(&request_id)?;
            let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
            let entries = repository
                .list_entries()
                .map_err(|error| error.to_string())?;
            let mut archive_names = entries
                .iter()
                .map(|entry| (entry.id.clone(), entry.name.clone()))
                .collect::<HashMap<_, _>>();
            for event in &events {
                if let EventKind::Published {
                    snapshot, entry, ..
                } = &event.kind
                {
                    archive_names
                        .entry(snapshot.entry_id.clone())
                        .or_insert_with(|| {
                            entry["name"]
                                .as_str()
                                .unwrap_or(&snapshot.entry_id)
                                .to_owned()
                        });
                }
            }
            let mut snapshots = Vec::new();
            let local_only = entries
                .iter()
                .filter(|entry| {
                    entry.storage_policy != chronicle_core::StoragePolicy::LocalAndRemote
                })
                .map(|entry| entry.id.clone())
                .collect::<BTreeSet<_>>();
            for entry in entries {
                if entry_ids.contains(&entry.id)
                    || entry_ids.is_empty()
                        && entry.storage_policy == chronicle_core::StoragePolicy::LocalAndRemote
                {
                    snapshots.extend(
                        repository
                            .list_snapshots(&entry.id)
                            .map_err(|error| error.to_string())?,
                    );
                }
            }
            let pending = pending(&repository)?;
            let operations = plan_sync(&snapshots, &events, &pending, false)
                .into_iter()
                .filter(|item| {
                    (if entry_ids.is_empty() {
                        !local_only.contains(&item.snapshot.entry_id)
                    } else {
                        entry_ids.contains(&item.snapshot.entry_id)
                    }) && snapshot_id
                        .as_ref()
                        .is_none_or(|id| *id == item.snapshot.id)
                })
                .map(|item| SyncOperation {
                    id: Uuid::new_v4().to_string(),
                    item,
                })
                .collect();
            Ok::<_, String>(SourcePlan {
                source_id: source_id.clone(),
                source_name: name,
                upgrade_required: remote_upgrade || !enabled(&root, &source_id),
                error: None,
                token: source_token(&root, &repository, &events, &signature)?,
                operations,
                archive_names,
                devices: known_devices(&events),
            })
        }
        .await;
        plan.sources.push(result.unwrap_or_else(|error| SourcePlan {
            source_id: source_id.clone(),
            source_name: source_id,
            upgrade_required: false,
            error: Some(error),
            token: String::new(),
            operations: Vec::new(),
            archive_names: HashMap::new(),
            devices: Vec::new(),
        }));
        let _ = app.emit(
            "snapshot-sync-progress",
            json!({"requestId":request_id,"sourcesChecked":plan.sources.len()}),
        );
    }
    end(&request_id);
    let mut saved = plans().lock().map_err(|_| "预览状态不可用")?;
    // Only the newest unopened plans are useful; never retain an unbounded cache.
    if saved.len() >= 16 {
        saved.clear();
    }
    saved.insert(plan.id.clone(), plan.clone());
    Ok(plan)
}

#[tauri::command(async)]
pub fn cancel_snapshot_sync(request_id: String) -> Result<(), String> {
    if let Some(task) = tasks()
        .lock()
        .map_err(|_| "同步任务状态不可用")?
        .get(&request_id)
    {
        task.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command(async)]
pub async fn enable_snapshot_sync_protocol(
    state: State<'_, AppState>,
    source_id: String,
    all_devices_upgraded: bool,
) -> Result<(), String> {
    let _writer = SourceWriteGuard::acquire(&source_id)?;
    if !all_devices_upgraded {
        return Err("请确认同一云端资料库的所有设备已升级；不支持旧版混用".into());
    }
    let (remote, root, _, _) = cloud::snapshot_remote(&state, &source_id)?;
    let (events, initializing) = remote_events(&remote).await?;
    if remote.optional_bytes("library.json").await?.is_none() {
        remote
            .put(
                "library.json",
                fs::read(root.join("library.json")).map_err(|error| error.to_string())?,
            )
            .await?;
    }
    match &remote {
        SnapshotRemote::Store(store) => store
            .test_capabilities()
            .await
            .map_err(|error| error.to_string())?,
        SnapshotRemote::GitHub(client) => client
            .test_access()
            .await
            .map_err(|error| error.to_string())?,
    }
    for event in &events {
        if let EventKind::Published {
            snapshot,
            object_path,
            ..
        } = &event.kind
            && initializing
        {
            let bytes = remote
                .optional_bytes(object_path)
                .await?
                .ok_or("已有快照文件缺失，不能初始化删除同步")?;
            if digest(&bytes) != snapshot.object_hash {
                return Err("已有快照校验失败，不能初始化删除同步".into());
            }
        }
        remote.append_event(event).await?;
    }
    let identity = {
        let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
        device(&repository)?
    };
    remote
        .append_event(&Event {
            operation_id: format!("device-{}-{}", identity.id, identity.revision),
            device: identity,
            kind: EventKind::DeviceNamed,
        })
        .await?;
    remote
        .put("sync-v2/protocol.json", serialize(&json!({"version":2}))?)
        .await?;
    write(
        &root,
        &source_state(&root, &source_id),
        &json!({"version":2,"sourceId":source_id}),
    )
}

async fn execute_operation(
    repository: &Arc<Mutex<LocalRepository>>,
    remote: &SnapshotRemote,
    root: &Path,
    op: &SyncOperation,
    events: &[Event],
) -> Result<(), String> {
    let snapshot = &op.item.snapshot;
    match op.item.action {
        Action::UploadRevision => {
            let event = op.item.revision.as_ref().ok_or("修订记录缺失")?;
            let ancestors = {
                let repo = repository.lock().map_err(|_| "资料库状态不可用")?;
                if repo
                    .get_snapshot(&snapshot.id)
                    .map_err(|error| error.to_string())?
                    != *snapshot
                {
                    return Err("节点已修改，请重新预览".into());
                }
                let EventKind::Revised { parents, .. } = &event.kind else {
                    return Err("修订记录格式无效".into());
                };
                let mut ancestors = repo
                    .snapshot_revision_records()
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .map(serde_json::from_value::<Event>)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?;
                let known_path = root.join("config/snapshot-known-revisions.json");
                if known_path.exists() {
                    ancestors.extend(parse::<Vec<Event>>(
                        &fs::read(known_path).map_err(|error| error.to_string())?,
                    )?);
                }
                ancestors.retain(|ancestor| {
                    parents.contains(&ancestor.operation_id)
                        && ancestor.snapshot().is_some_and(|item| {
                            item.id == snapshot.id && item.entry_id == snapshot.entry_id
                        })
                });
                if parents.iter().any(|id| {
                    !ancestors
                        .iter()
                        .chain(events)
                        .any(|ancestor| &ancestor.operation_id == id)
                }) {
                    return Err("修订依据缺失，请重新预览".into());
                }
                ancestors
            };
            if serialize(&remote_events(remote).await?.0)? != serialize(events)? {
                return Err("云端状态改变，请重新预览".into());
            }
            // This selected annotation includes its causal history, not other nodes.
            for ancestor in &ancestors {
                remote.append_event(ancestor).await?;
            }
            remote.append_event(event).await
        }
        Action::DownloadRevision => repository
            .lock()
            .map_err(|_| "资料库状态不可用")?
            .apply_cloud_snapshot_revision(
                op.item.local_snapshot.as_ref().ok_or("本机版本缺失")?,
                snapshot,
            )
            .map_err(|error| error.to_string()),
        Action::Upload => {
            let (bytes, identity, entry) = {
                let repository = repository.lock().map_err(|_| "资料库状态不可用")?;
                if repository
                    .list_snapshot_deletions()
                    .map_err(|error| error.to_string())?
                    .iter()
                    .any(|delete| delete.snapshot.id == snapshot.id)
                {
                    return Err("节点已删除，请重新预览".into());
                }
                let current = repository
                    .get_snapshot(&snapshot.id)
                    .map_err(|error| error.to_string())?;
                if current != *snapshot {
                    return Err("节点已修改，请重新预览".into());
                }
                let bytes = fs::read(
                    repository
                        .entry_storage_path(&snapshot.entry_id)
                        .map_err(|error| error.to_string())?
                        .join(&snapshot.archive_name),
                )
                .map_err(|error| error.to_string())?;
                let entry = catalog(root)?["entries"]
                    .as_array()
                    .and_then(|entries| {
                        entries
                            .iter()
                            .find(|entry| entry["id"].as_str() == Some(&snapshot.entry_id))
                    })
                    .cloned()
                    .ok_or("本地存档不存在")?;
                (bytes, device(&repository)?, entry)
            };
            if digest(&bytes) != snapshot.object_hash {
                return Err("本机快照校验失败".into());
            }
            let path = format!(
                "sync-v2/objects/{}-{}.7z",
                snapshot.id, snapshot.object_hash
            );
            remote.put(&path, bytes).await?;
            let bytes = remote
                .optional_bytes(&path)
                .await?
                .ok_or("上传后文件无法读取")?;
            if digest(&bytes) != snapshot.object_hash {
                return Err("上传后校验失败，尚未发布节点".into());
            }
            if serialize(&remote_events(remote).await?.0)? != serialize(events)? {
                return Err("上传过程中云端状态改变，请重新预览".into());
            }
            if repository
                .lock()
                .map_err(|_| "资料库状态不可用")?
                .get_snapshot(&snapshot.id)
                .map_err(|error| error.to_string())?
                != *snapshot
            {
                return Err("上传过程中节点改变，请重新预览".into());
            }
            let mut event = Event {
                operation_id: String::new(),
                device: identity,
                kind: EventKind::Published {
                    snapshot: snapshot.clone(),
                    object_path: path,
                    entry,
                },
            };
            event.operation_id = format!("publish-{}", digest(&serialize(&event)?));
            remote.append_event(&event).await
        }
        Action::Download => {
            let (path, metadata) = events
                .iter()
                .find_map(|event| match &event.kind {
                    EventKind::Published {
                        snapshot: item,
                        object_path,
                        entry,
                    } if item.id == snapshot.id => Some((object_path, entry)),
                    _ => None,
                })
                .ok_or("节点发布记录缺失")?;
            let bytes = remote
                .optional_bytes(path)
                .await?
                .ok_or("云端快照文件缺失")?;
            if serialize(&remote_events(remote).await?.0)? != serialize(events)? {
                return Err("下载过程中云端状态改变，请重新预览".into());
            }
            repository
                .lock()
                .map_err(|_| "资料库状态不可用")?
                .import_cloud_snapshot(metadata.clone(), snapshot.clone(), &bytes)
                .map_err(|error| error.to_string())
        }
        Action::RecycleRemote => {
            let event = op.item.deletion.as_ref().ok_or("删除记录缺失")?;
            let EventKind::Deleted {
                object_path,
                snapshot,
                ..
            } = &event.kind
            else {
                return Err("删除记录格式无效".into());
            };
            let recycled = root
                .join("recycle-snapshots")
                .join(format!("{}.7z", event.operation_id));
            let bytes = match fs::read(recycled) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let published = events
                        .iter()
                        .find_map(|event| match &event.kind {
                            EventKind::Published {
                                snapshot: item,
                                object_path,
                                ..
                            } if item.id == snapshot.id => Some(object_path),
                            _ => None,
                        })
                        .ok_or("本机回收文件已清理，云端也没有原始副本")?;
                    remote
                        .optional_bytes(published)
                        .await?
                        .ok_or("回收文件与原始云端副本均缺失")?
                }
                Err(error) => return Err(error.to_string()),
            };
            if digest(&bytes) != snapshot.object_hash {
                return Err("回收快照校验失败，不发布删除".into());
            }
            remote.put(object_path, bytes).await?;
            let bytes = remote
                .optional_bytes(object_path)
                .await?
                .ok_or("回收文件不可读取")?;
            if digest(&bytes) != snapshot.object_hash {
                return Err("云端回收快照校验失败".into());
            }
            if serialize(&remote_events(remote).await?.0)? != serialize(events)? {
                return Err("云端状态改变，尚未发布删除，请重新预览".into());
            }
            remote.append_event(event).await
        }
        Action::RecycleLocal => {
            let event = op.item.deletion.as_ref().ok_or("删除记录缺失")?;
            let EventKind::Deleted {
                object_path,
                snapshot,
                ..
            } = &event.kind
            else {
                return Err("删除记录格式无效".into());
            };
            let bytes = match remote.optional_bytes(object_path).await? {
                Some(bytes) => bytes,
                None => {
                    let repo = repository.lock().map_err(|_| "资料库状态不可用")?;
                    let path = repo
                        .entry_storage_path(&snapshot.entry_id)
                        .map_err(|error| error.to_string())?
                        .join(&snapshot.archive_name);
                    fs::read(path).map_err(|_| "云端回收文件与本机快照均不可读，保留节点")?
                }
            };
            if serialize(&remote_events(remote).await?.0)? != serialize(events)? {
                return Err("回收过程中云端状态改变，请重新预览".into());
            }
            let mut deletion = deletion_record(event)?;
            deletion.committed = false;
            let repo = repository.lock().map_err(|_| "资料库状态不可用")?;
            if let Some(base) = &op.item.local_snapshot
                && base != snapshot
            {
                repo.apply_cloud_snapshot_revision(base, snapshot)
                    .map_err(|error| error.to_string())?;
            }
            repo.accept_snapshot_deletion(deletion, &bytes)
                .map_err(|error| error.to_string())
        }
        _ => Err("此操作不能执行".into()),
    }
}

async fn refresh_remote_index(remote: &SnapshotRemote) -> Result<(), String> {
    let (events, _) = remote_events(remote).await?;
    let mut catalog: Value = match remote.optional_bytes("catalog.json").await? {
        Some(bytes) => parse(&bytes)?,
        None => json!({"format_version":4,"updated_at_ms":0,"categories":[],"entries":[]}),
    };
    let mut entries = catalog["entries"]
        .as_array()
        .cloned()
        .ok_or("目录索引无效")?;
    for event in &events {
        if let EventKind::Published {
            snapshot, entry, ..
        } = &event.kind
        {
            if !entries
                .iter()
                .any(|known| known["id"].as_str() == Some(&snapshot.entry_id))
            {
                entries.push(entry.clone());
            }
        }
    }
    let active = plan_sync(&[], &events, &[], false)
        .into_iter()
        .filter(|op| op.deletion.is_none() || op.action == Action::Conflict)
        .collect::<Vec<_>>();
    for entry in &mut entries {
        let timeline = active
            .iter()
            .filter(|op| entry["id"].as_str() == Some(&op.snapshot.entry_id))
            .map(|op| op.snapshot.clone())
            .collect::<Vec<_>>();
        entry["snapshots"] = serde_json::to_value(&timeline).map_err(|error| error.to_string())?;
        entry["snapshot_count"] = json!(timeline.len());
        entry["stored_bytes"] = json!(
            timeline
                .iter()
                .map(|snapshot| snapshot.size_bytes)
                .sum::<u64>()
        );
        entry["last_snapshot_at_ms"] =
            json!(timeline.iter().map(|snapshot| snapshot.created_at_ms).max());
    }
    catalog["entries"] = json!(entries);
    remote.put("catalog.json", serialize(&catalog)?).await
}

async fn record_success(
    state: &State<'_, AppState>,
    remote: &SnapshotRemote,
    root: &Path,
    source_id: &str,
    source_name: &str,
) -> Result<(), String> {
    let device = {
        let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
        device(&repository)?
    };
    remote
        .append_event(&Event {
            operation_id: format!("device-{}-{}", device.id, device.revision),
            device: device.clone(),
            kind: EventKind::DeviceNamed,
        })
        .await?;
    let at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as u64;
    remote
        .append_event(&Event {
            operation_id: Uuid::new_v4().to_string(),
            device,
            kind: EventKind::DeviceSynced { at_ms },
        })
        .await?;
    let events = remote_events(remote).await?.0;
    let observed_path = root.join("config/snapshot-observed-revisions.json");
    let mut observed: HashMap<String, Vec<String>> = if observed_path.exists() {
        parse(&fs::read(&observed_path).map_err(|error| error.to_string())?)?
    } else {
        HashMap::new()
    };
    {
        let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
        for event in &events {
            if let EventKind::Revised { snapshot, parents } = &event.kind {
                if repository
                    .get_snapshot(&snapshot.id)
                    .is_ok_and(|local| local == *snapshot)
                {
                    let ids = observed.entry(snapshot.id.clone()).or_default();
                    ids.push(event.operation_id.clone());
                    ids.extend(parents.iter().cloned());
                    ids.sort();
                    ids.dedup();
                }
            }
        }
    }
    write(root, &observed_path, &observed)?;
    let known_path = root.join("config/snapshot-known-revisions.json");
    let mut known: Vec<Event> = if known_path.exists() {
        parse(&fs::read(&known_path).map_err(|error| error.to_string())?)?
    } else {
        Vec::new()
    };
    for event in &events {
        if matches!(event.kind, EventKind::Revised { .. })
            && !known
                .iter()
                .any(|record| record.operation_id == event.operation_id)
        {
            known.push(event.clone());
        }
    }
    write(root, &known_path, &known)?;
    write(
        root,
        &root
            .join("config/sync-v2/devices")
            .join(format!("{}.json", digest(source_id.as_bytes()))),
        &json!({"sourceId":source_id,"sourceName":source_name,"devices":known_devices(&events)}),
    )
}

#[tauri::command(async)]
pub fn read_known_devices(state: State<'_, AppState>) -> Result<Vec<Value>, String> {
    let root = state
        .repository
        .lock()
        .map_err(|_| "资料库状态不可用")?
        .root()
        .join("config/sync-v2/devices");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut sources = Vec::new();
    for item in fs::read_dir(root).map_err(|error| error.to_string())? {
        let item = item.map_err(|error| error.to_string())?;
        if item
            .path()
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            sources.push(parse::<Value>(
                &fs::read(item.path()).map_err(|error| error.to_string())?,
            )?);
        }
    }
    Ok(sources)
}

#[tauri::command(async)]
pub async fn cloud_upload_new_snapshots(
    state: State<'_, AppState>,
    source_id: String,
    entry_id: String,
) -> Result<cloud::SyncResultDto, String> {
    let _writer = SourceWriteGuard::acquire(&source_id)?;
    let (remote, root, source_name, _) = cloud::snapshot_remote(&state, &source_id)?;
    let (events, upgrade_required) = remote_events(&remote).await?;
    require_protocol_ready(&root, &source_id, upgrade_required)?;
    let operations = {
        let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
        plan_sync(
            &repository
                .list_snapshots(&entry_id)
                .map_err(|error| error.to_string())?,
            &events,
            &pending(&repository)?,
            true,
        )
    };
    for item in operations {
        let (events, _) = remote_events(&remote).await?;
        // Recheck against deletions published while this upload queue was running.
        if events.iter().any(|event| matches!(&event.kind, EventKind::Deleted { snapshot, .. } if snapshot.id == item.snapshot.id)) { continue; }
        execute_operation(
            &state.repository,
            &remote,
            &root,
            &SyncOperation {
                id: Uuid::new_v4().to_string(),
                item,
            },
            &events,
        )
        .await?;
    }
    refresh_remote_index(&remote).await?;
    record_success(&state, &remote, &root, &source_id, &source_name).await?;
    Ok(cloud::SyncResultDto {
        status: "uploaded".into(),
        message: "已上传新增快照；未下载或同步删除".into(),
    })
}

#[tauri::command(async)]
pub async fn apply_snapshot_sync_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    plan_id: String,
    operation_ids: Vec<String>,
) -> Result<Vec<OperationResult>, String> {
    let plan = plans()
        .lock()
        .map_err(|_| "预览状态不可用")?
        .get(&plan_id)
        .cloned()
        .ok_or("预览已过期，请重新预览")?;
    let allowed = plan
        .sources
        .iter()
        .flat_map(|source| {
            source.operations.iter().map(|op| {
                (
                    op.id.clone(),
                    !matches!(op.item.action, Action::Conflict | Action::Unchanged),
                )
            })
        })
        .collect::<Vec<_>>();
    validate_selection(&allowed, &operation_ids)?;
    begin(&plan_id)?;
    let mut results = Vec::new();
    for source in &plan.sources {
        let selected = source
            .operations
            .iter()
            .filter(|op| operation_ids.contains(&op.id))
            .collect::<Vec<_>>();
        let writer = SourceWriteGuard::acquire(&source.source_id);
        let connection = writer
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|_| cloud::snapshot_remote(&state, &source.source_id));
        let mut expected = source.token.clone();
        if selected.is_empty() {
            let outcome = async {
                cancelled(&plan_id)?;
                if source.error.is_some() || source.upgrade_required {
                    return Err("此源未完成预览或尚未启用新协议".into());
                }
                let (remote, root, _, signature) = connection.as_ref().map_err(Clone::clone)?;
                let (events, _) = remote_events(remote).await?;
                {
                    let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
                    if source_token(root, &repository, &events, signature)? != expected {
                        return Err("本机或云端状态已变化，请重新预览".into());
                    }
                }
                // Repair only the projection of already committed records. Pending
                // and unchecked deletes are never appended here.
                refresh_remote_index(remote).await
            }
            .await;
            results.push(OperationResult {
                operation_id: "directory-index".into(),
                source_id: source.source_id.clone(),
                status: if outcome.is_ok() { "success" } else { "failed" }.into(),
                error: outcome.err(),
            });
        }
        for op in selected {
            let outcome = async {
                cancelled(&plan_id)?;
                if source.upgrade_required {
                    return Err("请先确认全部设备升级并启用此源，然后重新预览".into());
                }
                let (remote, root, _, signature) = connection.as_ref().map_err(Clone::clone)?;
                let (events, _) = remote_events(remote).await?;
                {
                    let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
                    if source_token(root, &repository, &events, signature)? != expected {
                        return Err("本机或云端状态已变化，请重新预览".into());
                    }
                }
                execute_operation(&state.repository, remote, root, op, &events).await?;
                refresh_remote_index(remote).await?;
                let (events, _) = remote_events(remote).await?;
                let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
                expected = source_token(root, &repository, &events, signature)?;
                Ok::<_, String>(())
            }
            .await;
            results.push(OperationResult {
                operation_id: op.id.clone(),
                source_id: source.source_id.clone(),
                status: if outcome.is_ok() { "success" } else { "failed" }.into(),
                error: outcome.err(),
            });
            let _ = app.emit(
                "snapshot-sync-progress",
                json!({"requestId":plan_id,"results":results}),
            );
        }
        if results
            .iter()
            .filter(|result| result.source_id == source.source_id)
            .all(|result| result.status == "success")
        {
            if let Ok((remote, root, name, _)) = &connection {
                if let Err(error) =
                    record_success(&state, remote, root, &source.source_id, name).await
                {
                    results.push(OperationResult {
                        operation_id: "device-record".into(),
                        source_id: source.source_id.clone(),
                        status: "failed".into(),
                        error: Some(error),
                    });
                }
            }
        }
    }
    end(&plan_id);
    plans()
        .lock()
        .map_err(|_| "预览状态不可用")?
        .remove(&plan_id);
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn recovery_from_cloud_after_parent_removal_keeps_no_foreign_bindings() {
        let (_temp, repo, entry, old) = fixture();
        let remote = test_remote();
        transfer(&repo, &remote, Action::Upload, &old.id)
            .await
            .unwrap();
        repo.lock()
            .unwrap()
            .delete_snapshot(&entry, &old.id)
            .unwrap();
        transfer(&repo, &remote, Action::RecycleRemote, &old.id)
            .await
            .unwrap();
        let op = repo.lock().unwrap().list_snapshot_deletions().unwrap()[0]
            .operation_id
            .clone();
        repo.lock().unwrap().purge_recycled_snapshot(&op).unwrap();
        repo.lock().unwrap().delete_entry(&entry, None).unwrap();
        restore_remote(&repo, &remote, &op).await.unwrap();
        assert_eq!(
            repo.lock().unwrap().list_snapshots(&entry).unwrap().len(),
            1
        );
        let temp = tempfile::tempdir().unwrap();
        let other = Arc::new(Mutex::new(
            LocalRepository::open(temp.path().join("repo")).unwrap(),
        ));
        restore_remote(&other, &remote, &op).await.unwrap();
        assert!(
            other
                .lock()
                .unwrap()
                .get_entry(&entry)
                .unwrap()
                .sources
                .is_empty()
        );
    }
    #[tokio::test]
    async fn uploading_a_revision_publishes_its_causal_ancestors() {
        let (_temp, repo, entry, old) = fixture();
        let remote = test_remote();
        transfer(&repo, &remote, Action::Upload, &old.id)
            .await
            .unwrap();
        {
            let repo = repo.lock().unwrap();
            repo.update_snapshot_note(&entry, &old.id, "first edit")
                .unwrap();
            repo.update_snapshot_note(&entry, &old.id, "second edit")
                .unwrap();
        }
        transfer(&repo, &remote, Action::UploadRevision, &old.id)
            .await
            .unwrap();
        let events = remote_events(&remote).await.unwrap().0;
        let own = repo.lock().unwrap().snapshot_revision_records().unwrap();
        assert!(own.iter().all(|record| {
            events
                .iter()
                .any(|event| Some(event.operation_id.as_str()) == record["operationId"].as_str())
        }));
    }
    #[tokio::test]
    async fn unlock_and_note_edits_before_deletion_travel_with_the_delete() {
        let (_temp, a, entry, original) = fixture();
        a.lock()
            .unwrap()
            .set_snapshot_locked(&entry, &original.id, true)
            .unwrap();
        let remote = test_remote();
        transfer(&a, &remote, Action::Upload, &original.id)
            .await
            .unwrap();
        let btemp = tempfile::tempdir().unwrap();
        let b = Arc::new(Mutex::new(
            LocalRepository::open(btemp.path().join("repo")).unwrap(),
        ));
        transfer(&b, &remote, Action::Download, &original.id)
            .await
            .unwrap();
        {
            let a = a.lock().unwrap();
            a.set_snapshot_locked(&entry, &original.id, false).unwrap();
            a.update_snapshot_note(&entry, &original.id, "before deletion")
                .unwrap();
            a.delete_snapshot(&entry, &original.id).unwrap();
        }
        transfer(&a, &remote, Action::RecycleRemote, &original.id)
            .await
            .unwrap();
        transfer(&b, &remote, Action::RecycleLocal, &original.id)
            .await
            .unwrap();
        assert!(b.lock().unwrap().get_snapshot(&original.id).is_err());
        assert_eq!(
            b.lock().unwrap().list_snapshot_deletions().unwrap()[0]
                .snapshot
                .note,
            "before deletion"
        );
    }
    #[test]
    fn automatic_upload_requires_local_consent_and_remote_protocol_marker() {
        let temp = tempfile::tempdir().unwrap();
        assert!(require_protocol_ready(temp.path(), "source", false).is_err());
        fs::create_dir_all(source_state(temp.path(), "source").parent().unwrap()).unwrap();
        fs::write(source_state(temp.path(), "source"), "{}").unwrap();
        assert!(require_protocol_ready(temp.path(), "source", true).is_err());
        assert!(require_protocol_ready(temp.path(), "source", false).is_ok());
    }
    fn test_remote() -> SnapshotRemote {
        let operator = opendal::Operator::new(opendal::services::Memory::default()).unwrap();
        SnapshotRemote::Store(chronicle_sync::RemoteStore::OpenDal(
            operator,
            Arc::new(tokio::sync::Mutex::new(tokio::time::Instant::now())),
            std::time::Duration::ZERO,
        ))
    }
    fn fixture() -> (
        tempfile::TempDir,
        Arc<Mutex<LocalRepository>>,
        String,
        Snapshot,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("save.txt");
        fs::write(&source, "one").unwrap();
        let repo = LocalRepository::open(temp.path().join("repo")).unwrap();
        let entry = repo.add_entry(&source, None, None).unwrap();
        let snapshot = repo
            .create_snapshot(&entry.id, "first", &repo.read_device().unwrap().id, false)
            .unwrap();
        (temp, Arc::new(Mutex::new(repo)), entry.id, snapshot)
    }
    async fn transfer(
        repo: &Arc<Mutex<LocalRepository>>,
        remote: &SnapshotRemote,
        action: Action,
        id: &str,
    ) -> Result<(), String> {
        let (events, _) = remote_events(remote).await?;
        let (root, local, deletes) = {
            let repo = repo.lock().unwrap();
            (
                repo.root().to_owned(),
                repo.list_entries()
                    .unwrap()
                    .iter()
                    .flat_map(|entry| repo.list_snapshots(&entry.id).unwrap())
                    .collect::<Vec<_>>(),
                pending(&repo)?,
            )
        };
        let item = plan_sync(&local, &events, &deletes, false)
            .into_iter()
            .find(|item| item.snapshot.id == id && item.action == action)
            .ok_or("action missing")?;
        execute_operation(
            repo,
            remote,
            &root,
            &SyncOperation {
                id: Uuid::new_v4().to_string(),
                item,
            },
            &events,
        )
        .await?;
        refresh_remote_index(remote).await
    }
    #[tokio::test]
    async fn two_devices_merge_new_nodes_and_explicit_delete_without_resurrection() {
        let (_a, a, entry, old) = fixture();
        let btemp = tempfile::tempdir().unwrap();
        let b = Arc::new(Mutex::new(
            LocalRepository::open(btemp.path().join("repo")).unwrap(),
        ));
        let remote = test_remote();
        transfer(&a, &remote, Action::Upload, &old.id)
            .await
            .unwrap();
        transfer(&b, &remote, Action::Download, &old.id)
            .await
            .unwrap();
        let aid = a.lock().unwrap().read_device().unwrap().id;
        let bid = b.lock().unwrap().read_device().unwrap().id;
        assert_ne!(aid, bid);
        // Bind this device's own source, not the source supplied by the cloud.
        assert!(
            b.lock()
                .unwrap()
                .get_entry(&entry)
                .unwrap()
                .sources
                .is_empty()
        );
        // Import a separately identified verified snapshot; no game directory restore is required.
        let (metadata, bytes) = {
            let repo = a.lock().unwrap();
            (
                catalog(repo.root()).unwrap()["entries"][0].clone(),
                fs::read(
                    repo.entry_storage_path(&entry)
                        .unwrap()
                        .join(&old.archive_name),
                )
                .unwrap(),
            )
        };
        let mut bnode = old.clone();
        bnode.id = Uuid::new_v4().to_string();
        bnode.archive_name = format!("{}.7z", bnode.id);
        bnode.device_id = bid;
        b.lock()
            .unwrap()
            .import_cloud_snapshot(metadata, bnode.clone(), &bytes)
            .unwrap();
        a.lock().unwrap().delete_snapshot(&entry, &old.id).unwrap();
        let anew = a
            .lock()
            .unwrap()
            .create_snapshot(&entry, "A new", &aid, false)
            .unwrap();
        transfer(&a, &remote, Action::Upload, &anew.id)
            .await
            .unwrap();
        transfer(&b, &remote, Action::Upload, &bnode.id)
            .await
            .unwrap();
        transfer(&a, &remote, Action::RecycleRemote, &old.id)
            .await
            .unwrap();
        transfer(&b, &remote, Action::RecycleLocal, &old.id)
            .await
            .unwrap();
        transfer(&b, &remote, Action::Download, &anew.id)
            .await
            .unwrap();
        transfer(&a, &remote, Action::Download, &bnode.id)
            .await
            .unwrap();
        for repo in [&a, &b] {
            let repo = repo.lock().unwrap();
            assert!(repo.get_snapshot(&old.id).is_err());
            assert!(repo.get_snapshot(&anew.id).is_ok());
            assert!(repo.get_snapshot(&bnode.id).is_ok());
            assert_eq!(
                plan_sync(
                    &repo.list_snapshots(&entry).unwrap(),
                    &remote_events(&remote).await.unwrap().0,
                    &pending(&repo).unwrap(),
                    true
                )
                .len(),
                0
            );
        }
        let bytes = remote
            .optional_bytes("catalog.json")
            .await
            .unwrap()
            .unwrap();
        let index: Value = parse(&bytes).unwrap();
        assert!(
            !index["entries"][0]["snapshots"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node["id"] == old.id)
        );
    }
    #[tokio::test]
    async fn interrupted_upload_stale_download_and_remote_restore_are_retryable() {
        let (_temp, repo, entry, old) = fixture();
        let remote = test_remote();
        let (root, item) = {
            let repo = repo.lock().unwrap();
            (
                repo.root().to_owned(),
                plan_sync(&[old.clone()], &[], &[], false).remove(0),
            )
        };
        let upload = SyncOperation {
            id: "test".into(),
            item,
        };
        // Another device publishes a record after preview: blob may be uploaded,
        // but publication must stop until re-previewed.
        remote
            .append_event(&Event {
                operation_id: "other-device".into(),
                device: Device {
                    id: "other".into(),
                    name: "Other".into(),
                    revision: 0,
                },
                kind: EventKind::DeviceNamed,
            })
            .await
            .unwrap();
        assert!(
            execute_operation(&repo, &remote, &root, &upload, &[])
                .await
                .is_err()
        );
        assert!(
            !remote_events(&remote)
                .await
                .unwrap()
                .0
                .iter()
                .any(|event| event.snapshot().is_some())
        );
        transfer(&repo, &remote, Action::Upload, &old.id)
            .await
            .unwrap();
        let stale = remote_events(&remote).await.unwrap().0;
        let other_temp = tempfile::tempdir().unwrap();
        let other = Arc::new(Mutex::new(
            LocalRepository::open(other_temp.path().join("repo")).unwrap(),
        ));
        let download = SyncOperation {
            id: "download".into(),
            item: plan_sync(&[], &stale, &[], false).remove(0),
        };
        repo.lock()
            .unwrap()
            .delete_snapshot(&entry, &old.id)
            .unwrap();
        transfer(&repo, &remote, Action::RecycleRemote, &old.id)
            .await
            .unwrap();
        assert!(
            execute_operation(&other, &remote, other_temp.path(), &download, &stale)
                .await
                .is_err()
        );
        assert!(other.lock().unwrap().list_entries().unwrap().is_empty());
        let op = repo.lock().unwrap().list_snapshot_deletions().unwrap()[0]
            .operation_id
            .clone();
        repo.lock().unwrap().purge_recycled_snapshot(&op).unwrap();
        // Local cleanup must not prevent recovery from an unpurged cloud copy.
        restore_remote(&repo, &remote, &op).await.unwrap();
        restore_remote(&other, &remote, &op).await.unwrap();
        restore_remote(&other, &remote, &op).await.unwrap();
        let other = other.lock().unwrap();
        let nodes = other.list_snapshots(&entry).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_ne!(nodes[0].id, old.id);
        assert_eq!(nodes[0].parent_id.as_deref(), Some(old.id.as_str()));
        assert_eq!(other.list_snapshot_deletions().unwrap()[0].operation_id, op);
    }
    #[test]
    fn cancellation_is_task_scoped_and_does_not_erase_completed_work() {
        begin("cancel-test").unwrap();
        begin("other-test").unwrap();
        cancel_snapshot_sync("cancel-test".into()).unwrap();
        assert!(cancelled("cancel-test").is_err());
        assert!(cancelled("other-test").is_ok());
        end("cancel-test");
        end("other-test");
    }
    #[tokio::test]
    async fn protocol_marker_without_records_is_not_a_no_change_source() {
        let remote = test_remote();
        remote
            .put(
                "sync-v2/protocol.json",
                serialize(&json!({"version":2})).unwrap(),
            )
            .await
            .unwrap();
        assert!(remote_events(&remote).await.is_err());
    }
    #[tokio::test]
    async fn an_offline_device_can_recycle_its_verified_copy_after_cloud_purge() {
        let (_temp, a, entry, old) = fixture();
        let remote = test_remote();
        transfer(&a, &remote, Action::Upload, &old.id)
            .await
            .unwrap();
        let btemp = tempfile::tempdir().unwrap();
        let b = Arc::new(Mutex::new(
            LocalRepository::open(btemp.path().join("repo")).unwrap(),
        ));
        transfer(&b, &remote, Action::Download, &old.id)
            .await
            .unwrap();
        a.lock().unwrap().delete_snapshot(&entry, &old.id).unwrap();
        transfer(&a, &remote, Action::RecycleRemote, &old.id)
            .await
            .unwrap();
        let events = remote_events(&remote).await.unwrap().0;
        for event in &events {
            match &event.kind {
                EventKind::Published { object_path, .. }
                | EventKind::Deleted { object_path, .. } => {
                    remote.delete(object_path).await.unwrap()
                }
                _ => {}
            }
        }
        transfer(&b, &remote, Action::RecycleLocal, &old.id)
            .await
            .unwrap();
        assert!(b.lock().unwrap().get_snapshot(&old.id).is_err());
        assert!(b.lock().unwrap().list_snapshot_deletions().unwrap()[0].committed);
    }
    #[test]
    fn writes_to_one_source_are_serialized_but_other_sources_are_independent() {
        let first = SourceWriteGuard::acquire("test-source-a").unwrap();
        assert!(SourceWriteGuard::acquire("test-source-a").is_err());
        assert!(SourceWriteGuard::acquire("test-source-b").is_ok());
        drop(first);
        assert!(SourceWriteGuard::acquire("test-source-a").is_ok());
    }
    #[test]
    fn deletion_round_trip_preserves_actor_and_time() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("save.txt");
        fs::write(&source, "save").unwrap();
        let repo = LocalRepository::open(temp.path().join("repo")).unwrap();
        let entry = repo.add_entry(&source, None, None).unwrap();
        let snapshot = repo
            .create_snapshot(&entry.id, "first", &repo.read_device().unwrap().id, false)
            .unwrap();
        repo.delete_snapshot(&entry.id, &snapshot.id).unwrap();
        let record = repo.list_snapshot_deletions().unwrap().remove(0);
        let event = pending(&repo).unwrap().remove(0);
        let EventKind::Deleted { deleted_at_ms, .. } = event.kind else {
            panic!("delete expected")
        };
        assert_eq!(deleted_at_ms, record.deleted_at_ms);
        assert!(deleted_at_ms > 0);
    }
    #[test]
    fn validation_token_changes_for_remote_records_local_catalog_and_pending_deletes() {
        let base = validation_token(b"catalog", b"deletes", b"remote", b"source");
        assert_ne!(
            base,
            validation_token(b"catalog changed", b"deletes", b"remote", b"source")
        );
        assert_ne!(
            base,
            validation_token(b"catalog", b"deletes changed", b"remote", b"source")
        );
        assert_ne!(
            base,
            validation_token(b"catalog", b"deletes", b"remote changed", b"source")
        );
        assert_ne!(
            base,
            validation_token(b"catalog", b"deletes", b"remote", b"source changed")
        );
    }
    #[test]
    fn selection_cannot_expand_the_preview_scope_or_execute_conflicts() {
        let allowed = vec![("upload".to_owned(), true), ("conflict".to_owned(), false)];
        assert!(validate_selection(&allowed, &["upload".into()]).is_ok());
        assert!(validate_selection(&allowed, &["unknown".into()]).is_err());
        assert!(validate_selection(&allowed, &["conflict".into()]).is_err());
        assert!(validate_selection(&allowed, &["upload".into(), "upload".into()]).is_err());
    }
}
