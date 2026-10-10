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
use tauri::{AppHandle, Emitter, Manager, State};
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
    index_repair_id: Option<String>,
    device_record_id: Option<String>,
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
#[derive(Default)]
struct TaskCancellation {
    cancelled: AtomicBool,
    readers: Mutex<HashMap<Uuid, std::task::Waker>>,
}
struct ReadRegistration(Arc<TaskCancellation>, Uuid);
impl Drop for ReadRegistration {
    fn drop(&mut self) {
        if let Ok(mut readers) = self.0.readers.lock() {
            readers.remove(&self.1);
        }
    }
}
static TASKS: OnceLock<Mutex<HashMap<String, Arc<TaskCancellation>>>> = OnceLock::new();
static EARLY_CANCELLATIONS: OnceLock<Mutex<std::collections::VecDeque<String>>> = OnceLock::new();
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
fn tasks() -> &'static Mutex<HashMap<String, Arc<TaskCancellation>>> {
    TASKS.get_or_init(Mutex::default)
}
fn cancelled(id: &str) -> Result<(), String> {
    if tasks()
        .lock()
        .map_err(|_| "同步任务状态不可用")?
        .get(id)
        .is_some_and(|cancel| cancel.cancelled.load(Ordering::Relaxed))
    {
        Err("已取消；已提交的操作不会撤销".into())
    } else {
        Ok(())
    }
}
pub(crate) fn begin(id: &str) -> Result<(), String> {
    let mut tasks = tasks().lock().map_err(|_| "同步任务状态不可用")?;
    if tasks.contains_key(id) {
        return Err("同步任务已运行".into());
    }
    let task = TaskCancellation::default();
    let mut early = EARLY_CANCELLATIONS
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| "同步任务状态不可用")?;
    if let Some(index) = early.iter().position(|pending| pending == id) {
        early.remove(index);
        task.cancelled.store(true, Ordering::Relaxed);
    }
    tasks.insert(id.into(), Arc::new(task));
    Ok(())
}
pub(crate) fn end(id: &str) {
    if let Ok(mut tasks) = tasks().lock() {
        tasks.remove(id);
    }
}

pub(crate) async fn cancellable_read<T>(
    id: &str,
    read: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let task = tasks()
        .lock()
        .map_err(|_| "同步任务状态不可用")?
        .get(id)
        .cloned();
    let Some(task) = task else { return read.await };
    let registration = ReadRegistration(task.clone(), Uuid::new_v4());
    let mut read = std::pin::pin!(read);
    std::future::poll_fn(|context| {
        let Ok(mut readers) = task.readers.lock() else {
            return std::task::Poll::Ready(Err("同步任务状态不可用".into()));
        };
        readers.insert(registration.1, context.waker().clone());
        if task.cancelled.load(Ordering::Relaxed) {
            return std::task::Poll::Ready(Err("已取消；已提交的操作不会撤销".into()));
        }
        drop(readers);
        read.as_mut().poll(context)
    })
    .await
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

struct SyncSession {
    initial: LocalSyncVersion,
    expected: Mutex<String>,
}
struct LocalSyncVersion {
    root: PathBuf,
    catalog: Vec<u8>,
    deletes: Vec<u8>,
    identity: chronicle_storage::DeviceIdentity,
}
impl LocalSyncVersion {
    fn read(repository: &LocalRepository) -> Result<Self, String> {
        Ok(Self {
            root: repository.root().to_owned(),
            catalog: fs::read(repository.root().join("catalog.json"))
                .map_err(|error| error.to_string())?,
            deletes: serialize(
                &repository
                    .list_snapshot_deletions()
                    .map_err(|error| error.to_string())?,
            )?,
            identity: repository
                .read_device()
                .map_err(|error| error.to_string())?,
        })
    }
    fn fingerprint(&self) -> Result<String, String> {
        Ok(validation_token(
            &self.catalog,
            &self.deletes,
            &[],
            &serialize(&(&self.root, &self.identity))?,
        ))
    }
    fn source_token(&self, events: &[Event], signature: &[u8]) -> Result<String, String> {
        Ok(validation_token(
            &self.catalog,
            &self.deletes,
            &serialize(events)?,
            &serialize(&(signature, self.root.to_string_lossy(), &self.identity))?,
        ))
    }
}
impl SyncSession {
    fn new(repository: &LocalRepository) -> Result<Self, String> {
        let initial = LocalSyncVersion::read(repository)?;
        Ok(Self {
            expected: Mutex::new(initial.fingerprint()?),
            initial,
        })
    }
    fn check_local(&self, repository: &LocalRepository) -> Result<(), String> {
        if LocalSyncVersion::read(repository)?.fingerprint()?
            != *self.expected.lock().map_err(|_| "同步状态不可用")?
        {
            return Err("本机或云端状态已变化，请重新预览".into());
        }
        Ok(())
    }
    fn validate(
        &self,
        repository: &LocalRepository,
        root: &Path,
        events: &[Event],
        signature: &[u8],
        expected: &str,
    ) -> Result<(), String> {
        self.check_local(repository)?;
        if root != self.initial.root || self.initial.source_token(events, signature)? != expected {
            return Err("本机或云端状态已变化，请重新预览".into());
        }
        Ok(())
    }
    fn commit<T>(
        &self,
        repository: &LocalRepository,
        operation: impl FnOnce(&LocalRepository) -> Result<T, String>,
    ) -> Result<T, String> {
        self.check_local(repository)?;
        let result = operation(repository);
        *self.expected.lock().map_err(|_| "同步状态不可用")? =
            LocalSyncVersion::read(repository)?.fingerprint()?;
        result
    }
}

async fn run_selected_sources<F, Fut>(
    sources: Vec<SourcePlan>,
    selected: &BTreeSet<String>,
    run: F,
) -> Result<Vec<OperationResult>, String>
where
    F: Fn(SourcePlan) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = Vec<OperationResult>> + Send + 'static,
{
    let mut workers = Vec::new();
    for source in sources {
        if source_has_selected_work(&source, selected) {
            workers.push(tauri::async_runtime::spawn(run(source)));
        }
    }
    let mut results = Vec::new();
    let mut failure = None;
    for worker in workers {
        match worker.await {
            Ok(items) => results.extend(items),
            Err(error) => {
                failure = Some(error.to_string());
            }
        }
    }
    failure.map_or(Ok(results), Err)
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
            let (events, remote_upgrade) =
                cancellable_read(&request_id, remote_events(&remote)).await?;
            let index_repair_id =
                if index_repair_needed(&remote, &events, Some(&request_id)).await? {
                    Some(Uuid::new_v4().to_string())
                } else {
                    None
                };
            cancelled(&request_id)?;
            let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
            let entries = repository
                .list_entries_with_snapshots()
                .map_err(|error| error.to_string())?;
            let mut archive_names = entries
                .iter()
                .map(|(entry, _)| (entry.id.clone(), entry.name.clone()))
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
                .filter(|(entry, _)| {
                    entry.storage_policy != chronicle_core::StoragePolicy::LocalAndRemote
                })
                .map(|(entry, _)| entry.id.clone())
                .collect::<BTreeSet<_>>();
            for (entry, timeline) in entries {
                if entry_ids.contains(&entry.id)
                    || entry_ids.is_empty()
                        && entry.storage_policy == chronicle_core::StoragePolicy::LocalAndRemote
                {
                    snapshots.extend(timeline);
                }
            }
            let pending = pending(&repository)?;
            let device_record_id = device_record_needed(&device(&repository)?, &events)
                .then(|| Uuid::new_v4().to_string());
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
                index_repair_id,
                device_record_id,
                archive_names,
                devices: known_devices(&events),
            })
        }
        .await;
        if let Err(error) = cancelled(&request_id) {
            end(&request_id);
            return Err(error);
        }
        plan.sources.push(result.unwrap_or_else(|error| SourcePlan {
            source_id: source_id.clone(),
            source_name: source_id,
            upgrade_required: false,
            error: Some(error),
            token: String::new(),
            operations: Vec::new(),
            index_repair_id: None,
            device_record_id: None,
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
    let tasks = tasks().lock().map_err(|_| "同步任务状态不可用")?;
    if let Some(task) = tasks.get(&request_id) {
        task.cancelled.store(true, Ordering::Relaxed);
        let readers = std::mem::take(&mut *task.readers.lock().map_err(|_| "同步任务状态不可用")?);
        for reader in readers.into_values() {
            reader.wake();
        }
    } else {
        // IPC cancellation may arrive before the async preview starts. Bound
        // these short-lived IDs so a late cancellation cannot grow the registry.
        let mut early = EARLY_CANCELLATIONS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| "同步任务状态不可用")?;
        if !early.contains(&request_id) {
            if early.len() == 64 {
                early.pop_front();
            }
            early.push_back(request_id);
        }
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

fn prepare_upload_payload(
    repository: &Arc<Mutex<LocalRepository>>,
    root: &Path,
    snapshot: &Snapshot,
    read_file: impl FnOnce(&Path) -> std::io::Result<Vec<u8>>,
) -> Result<(Vec<u8>, Device, Value), String> {
    let (path, identity, entry) = {
        let repository = repository.lock().map_err(|_| "资料库状态不可用")?;
        if repository
            .list_snapshot_deletions()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|delete| delete.snapshot.id == snapshot.id)
        {
            return Err("节点已删除，请重新预览".into());
        }
        if repository
            .get_snapshot(&snapshot.id)
            .map_err(|error| error.to_string())?
            != *snapshot
        {
            return Err("节点已修改，请重新预览".into());
        }
        let path = repository
            .entry_storage_path(&snapshot.entry_id)
            .map_err(|error| error.to_string())?
            .join(&snapshot.archive_name);
        let entry = catalog(root)?["entries"]
            .as_array()
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry["id"].as_str() == Some(&snapshot.entry_id))
            })
            .cloned()
            .ok_or("本地存档不存在")?;
        (path, device(&repository)?, entry)
    };
    let bytes = read_file(&path).map_err(|error| error.to_string())?;
    Ok((bytes, identity, entry))
}

async fn execute_operation(
    repository: &Arc<Mutex<LocalRepository>>,
    remote: &SnapshotRemote,
    root: &Path,
    op: &SyncOperation,
    events: &[Event],
    request_id: Option<&str>,
    session: Option<&SyncSession>,
) -> Result<Vec<Event>, String> {
    let request_id = request_id.unwrap_or("");
    cancelled(request_id)?;
    if let Some(session) = session {
        session.check_local(&*repository.lock().map_err(|_| "资料库状态不可用")?)?;
    }
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
            if serialize(&cancellable_read(request_id, remote_events(remote)).await?.0)?
                != serialize(events)?
            {
                return Err("云端状态改变，请重新预览".into());
            }
            // This selected annotation includes its causal history, not other nodes.
            for ancestor in &ancestors {
                cancelled(request_id)?;
                remote.append_event(ancestor).await?;
            }
            cancelled(request_id)?;
            check_sync_local(repository, session)?;
            remote.append_event(event).await?;
            Ok(ancestors
                .into_iter()
                .chain(std::iter::once(event.clone()))
                .collect())
        }
        Action::DownloadRevision => commit_sync_local(repository, session, |repo| {
            if repo
                .get_snapshot(&snapshot.id)
                .is_ok_and(|current| current == *snapshot)
            {
                return Ok(());
            }
            repo.apply_cloud_snapshot_revision(
                op.item.local_snapshot.as_ref().ok_or("本机版本缺失")?,
                snapshot,
            )
            .map_err(|error| error.to_string())
        })
        .map(|_| Vec::new()),
        Action::Upload => {
            let upload_repository = Arc::clone(repository);
            let upload_root = root.to_owned();
            let upload_snapshot = snapshot.clone();
            let (bytes, identity, entry) = tauri::async_runtime::spawn_blocking(move || {
                prepare_upload_payload(&upload_repository, &upload_root, &upload_snapshot, |path| {
                    fs::read(path)
                })
            })
            .await
            .map_err(|error| error.to_string())??;
            cancelled(request_id)?;
            if digest(&bytes) != snapshot.object_hash {
                return Err("本机快照校验失败".into());
            }
            let path = format!(
                "sync-v2/objects/{}-{}.7z",
                snapshot.id, snapshot.object_hash
            );
            remote.put(&path, bytes).await?;
            let bytes = cancellable_read(request_id, remote.optional_bytes(&path))
                .await?
                .ok_or("上传后文件无法读取")?;
            if digest(&bytes) != snapshot.object_hash {
                return Err("上传后校验失败，尚未发布节点".into());
            }
            if serialize(&cancellable_read(request_id, remote_events(remote)).await?.0)?
                != serialize(events)?
            {
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
            check_sync_local(repository, session)?;
            remote.append_event(&event).await?;
            Ok(vec![event])
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
            let bytes = cancellable_read(request_id, remote.optional_bytes(path))
                .await?
                .ok_or("云端快照文件缺失")?;
            if serialize(&cancellable_read(request_id, remote_events(remote)).await?.0)?
                != serialize(events)?
            {
                return Err("下载过程中云端状态改变，请重新预览".into());
            }
            commit_sync_local(repository, session, |repo| {
                repo.import_cloud_snapshot(metadata.clone(), snapshot.clone(), &bytes)
                    .map_err(|error| error.to_string())
            })
            .map(|_| Vec::new())
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
                    cancellable_read(request_id, remote.optional_bytes(published))
                        .await?
                        .ok_or("回收文件与原始云端副本均缺失")?
                }
                Err(error) => return Err(error.to_string()),
            };
            if digest(&bytes) != snapshot.object_hash {
                return Err("回收快照校验失败，不发布删除".into());
            }
            remote.put(object_path, bytes).await?;
            let bytes = cancellable_read(request_id, remote.optional_bytes(object_path))
                .await?
                .ok_or("回收文件不可读取")?;
            if digest(&bytes) != snapshot.object_hash {
                return Err("云端回收快照校验失败".into());
            }
            if serialize(&cancellable_read(request_id, remote_events(remote)).await?.0)?
                != serialize(events)?
            {
                return Err("云端状态改变，尚未发布删除，请重新预览".into());
            }
            check_sync_local(repository, session)?;
            remote.append_event(event).await?;
            Ok(vec![event.clone()])
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
            let bytes =
                match cancellable_read(request_id, remote.optional_bytes(object_path)).await? {
                    Some(bytes) => bytes,
                    None => {
                        let repo = repository.lock().map_err(|_| "资料库状态不可用")?;
                        if let Some(session) = session {
                            session.check_local(&repo)?;
                        }
                        if repo
                            .list_snapshot_deletions()
                            .map_err(|error| error.to_string())?
                            .iter()
                            .any(|item| item.operation_id == event.operation_id)
                        {
                            return Ok(Vec::new());
                        }
                        let path = repo
                            .entry_storage_path(&snapshot.entry_id)
                            .map_err(|error| error.to_string())?
                            .join(&snapshot.archive_name);
                        fs::read(path).map_err(|_| "云端回收文件与本机快照均不可读，保留节点")?
                    }
                };
            if serialize(&cancellable_read(request_id, remote_events(remote)).await?.0)?
                != serialize(events)?
            {
                return Err("回收过程中云端状态改变，请重新预览".into());
            }
            let mut deletion = deletion_record(event)?;
            deletion.committed = false;
            commit_sync_local(repository, session, |repo| {
                if repo
                    .list_snapshot_deletions()
                    .map_err(|error| error.to_string())?
                    .iter()
                    .any(|item| item.operation_id == deletion.operation_id)
                {
                    return Ok(());
                }
                if let Some(base) = &op.item.local_snapshot
                    && base != snapshot
                    && !repo
                        .get_snapshot(&snapshot.id)
                        .is_ok_and(|current| current == *snapshot)
                {
                    repo.apply_cloud_snapshot_revision(base, snapshot)
                        .map_err(|error| error.to_string())?;
                }
                repo.accept_snapshot_deletion(deletion, &bytes)
                    .map_err(|error| error.to_string())
            })
            .map(|_| Vec::new())
        }
        _ => Err("此操作不能执行".into()),
    }
}

fn check_sync_local(
    repository: &Arc<Mutex<LocalRepository>>,
    session: Option<&SyncSession>,
) -> Result<(), String> {
    if let Some(session) = session {
        session.check_local(&*repository.lock().map_err(|_| "资料库状态不可用")?)?;
    }
    Ok(())
}

fn commit_sync_local<T>(
    repository: &Arc<Mutex<LocalRepository>>,
    session: Option<&SyncSession>,
    operation: impl FnOnce(&LocalRepository) -> Result<T, String>,
) -> Result<T, String> {
    let repo = repository.lock().map_err(|_| "资料库状态不可用")?;
    match session {
        Some(session) => session.commit(&repo, operation),
        None => operation(&repo),
    }
}

fn validate_remote_commit(
    before: &[Event],
    committed: &[Event],
    after: &[Event],
) -> Result<(), String> {
    let expected = before
        .iter()
        .chain(committed)
        .map(|event| Ok((event.operation_id.clone(), serialize(event)?)))
        .collect::<Result<HashMap<_, _>, String>>()?;
    let actual = after
        .iter()
        .map(|event| Ok((event.operation_id.clone(), serialize(event)?)))
        .collect::<Result<HashMap<_, _>, String>>()?;
    if actual != expected {
        return Err("云端状态改变，请重新预览".into());
    }
    Ok(())
}

async fn refresh_remote_index(remote: &SnapshotRemote) -> Result<(), String> {
    let (events, _) = remote_events(remote).await?;
    refresh_remote_index_from_events(remote, &events, None).await
}
async fn index_repair_needed(
    remote: &SnapshotRemote,
    events: &[Event],
    request_id: Option<&str>,
) -> Result<bool, String> {
    let bytes = cancellable_read(
        request_id.unwrap_or(""),
        remote.optional_bytes("catalog.json"),
    )
    .await?;
    let Some(bytes) = bytes else {
        return Ok(events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Published { .. })));
    };
    let catalog: Value = parse(&bytes)?;
    Ok(project_remote_index(catalog.clone(), events)? != catalog)
}
fn source_has_selected_work(source: &SourcePlan, selected: &BTreeSet<String>) -> bool {
    source.operations.iter().any(|op| selected.contains(&op.id))
        || source
            .index_repair_id
            .as_ref()
            .is_some_and(|id| selected.contains(id))
        || source
            .device_record_id
            .as_ref()
            .is_some_and(|id| selected.contains(id))
}
fn device_record_needed(identity: &Device, events: &[Event]) -> bool {
    !known_devices(events).iter().any(|known| {
        known.id == identity.id
            && (known.revision > identity.revision
                || known.revision == identity.revision && known.name == identity.name)
    })
}

async fn refresh_remote_index_from_events(
    remote: &SnapshotRemote,
    events: &[Event],
    request_id: Option<&str>,
) -> Result<(), String> {
    let request_id = request_id.unwrap_or("");
    let catalog: Value =
        match cancellable_read(request_id, remote.optional_bytes("catalog.json")).await? {
            Some(bytes) => parse(&bytes)?,
            None => json!({"format_version":4,"updated_at_ms":0,"categories":[],"entries":[]}),
        };
    let catalog = project_remote_index(catalog, events)?;
    cancelled(request_id)?;
    remote.put("catalog.json", serialize(&catalog)?).await
}

fn projection_entry_id(entry: &Value) -> Option<&str> {
    #[cfg(test)]
    tests::PROJECTION_ENTRY_READS.with(|count| count.set(count.get() + 1));
    entry["id"].as_str()
}
fn project_remote_index(mut catalog: Value, events: &[Event]) -> Result<Value, String> {
    let mut entries = catalog["entries"]
        .as_array()
        .cloned()
        .ok_or("目录索引无效")?;
    let mut entry_ids = entries
        .iter()
        .filter_map(projection_entry_id)
        .map(str::to_owned)
        .collect::<std::collections::HashSet<_>>();
    for event in events {
        if let EventKind::Published {
            snapshot, entry, ..
        } = &event.kind
        {
            if !entry_ids.contains(&snapshot.entry_id) {
                if let Some(id) = projection_entry_id(entry) {
                    entry_ids.insert(id.to_owned());
                }
                entries.push(entry.clone());
            }
        }
    }
    let mut timelines = HashMap::<String, Vec<Snapshot>>::new();
    for op in plan_sync(&[], events, &[], false) {
        if op.deletion.is_none() || op.action == Action::Conflict {
            timelines
                .entry(op.snapshot.entry_id.clone())
                .or_default()
                .push(op.snapshot);
        }
    }
    for entry in &mut entries {
        let timeline = projection_entry_id(entry)
            .and_then(|id| timelines.get(id))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
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
    Ok(catalog)
}

async fn record_success(
    state: &State<'_, AppState>,
    remote: &SnapshotRemote,
    root: &Path,
    source_id: &str,
    source_name: &str,
    request_id: Option<&str>,
) -> Result<(), String> {
    let request_id = request_id.unwrap_or("");
    cancelled(request_id)?;
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
    cancelled(request_id)?;
    remote
        .append_event(&Event {
            operation_id: Uuid::new_v4().to_string(),
            device,
            kind: EventKind::DeviceSynced { at_ms },
        })
        .await?;
    let events = cancellable_read(request_id, remote_events(remote)).await?.0;
    merge_source_records(&state.repository, root, source_id, source_name, &events)
}

fn merge_source_records(
    repository: &Arc<Mutex<LocalRepository>>,
    root: &Path,
    source_id: &str,
    source_name: &str,
    events: &[Event],
) -> Result<(), String> {
    // Keep the shared read/merge/write together, never the remote requests.
    let repository = repository.lock().map_err(|_| "资料库状态不可用")?;
    if repository.root() != root {
        return Err("资料库位置已改变，请重新预览".into());
    }
    let observed_path = root.join("config/snapshot-observed-revisions.json");
    let mut observed: HashMap<String, Vec<String>> = if observed_path.exists() {
        parse(&fs::read(&observed_path).map_err(|error| error.to_string())?)?
    } else {
        HashMap::new()
    };
    let snapshots = if events
        .iter()
        .any(|event| matches!(event.kind, EventKind::Revised { .. }))
    {
        let mut snapshots = HashMap::new();
        for snapshot in repository
            .list_snapshot_manifests()
            .map_err(|error| error.to_string())?
        {
            snapshots.entry(snapshot.id.clone()).or_insert(snapshot);
        }
        snapshots
    } else {
        HashMap::new()
    };
    let mut revised_ids = BTreeSet::new();
    for event in events {
        if let EventKind::Revised { snapshot, parents } = &event.kind {
            if snapshots.get(&snapshot.id) == Some(snapshot) {
                let ids = observed.entry(snapshot.id.clone()).or_default();
                ids.push(event.operation_id.clone());
                ids.extend(parents.iter().cloned());
                revised_ids.insert(snapshot.id.clone());
            }
        }
    }
    for id in revised_ids {
        let ids = observed
            .get_mut(&id)
            .expect("matching revision inserted above");
        #[cfg(test)]
        tests::OBSERVED_SORTS.with(|count| count.set(count.get() + 1));
        ids.sort();
        ids.dedup();
    }
    write(root, &observed_path, &observed)?;
    let known_path = root.join("config/snapshot-known-revisions.json");
    let mut known: Vec<Event> = if known_path.exists() {
        parse(&fs::read(&known_path).map_err(|error| error.to_string())?)?
    } else {
        Vec::new()
    };
    let mut known_ids = known
        .iter()
        .map(|event| event.operation_id.clone())
        .collect::<BTreeSet<_>>();
    for event in events {
        if matches!(event.kind, EventKind::Revised { .. })
            && known_ids.insert(event.operation_id.clone())
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
            None,
            None,
        )
        .await?;
    }
    refresh_remote_index(&remote).await?;
    record_success(&state, &remote, &root, &source_id, &source_name, None).await?;
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
            source
                .operations
                .iter()
                .map(|op| {
                    (
                        op.id.clone(),
                        !matches!(op.item.action, Action::Conflict | Action::Unchanged),
                    )
                })
                .chain(source.index_repair_id.iter().map(|id| (id.clone(), true)))
                .chain(source.device_record_id.iter().map(|id| (id.clone(), true)))
        })
        .collect::<Vec<_>>();
    validate_selection(&allowed, &operation_ids)?;
    let session = Arc::new(SyncSession::new(
        &*state.repository.lock().map_err(|_| "资料库状态不可用")?,
    )?);
    begin(&plan_id)?;
    let selected_ids = Arc::new(operation_ids.into_iter().collect::<BTreeSet<_>>());
    let worker_ids = selected_ids.clone();
    let worker_plan_id = plan_id.clone();
    let results = run_selected_sources(plan.sources, &selected_ids, move |source| {
        let app = app.clone();
        let session = session.clone();
        let selected_ids = worker_ids.clone();
        let plan_id = worker_plan_id.clone();
        async move {
            let state = app.state::<AppState>();
            apply_source_plan(&app, &state, &source, &selected_ids, &plan_id, &session).await
        }
    })
    .await;
    end(&plan_id);
    plans()
        .lock()
        .map_err(|_| "预览状态不可用")?
        .remove(&plan_id);
    results
}

async fn apply_source_plan(
    app: &AppHandle,
    state: &State<'_, AppState>,
    source: &SourcePlan,
    selected_ids: &BTreeSet<String>,
    plan_id: &str,
    session: &SyncSession,
) -> Vec<OperationResult> {
    let mut results = Vec::new();
    let _ = app.emit(
        "snapshot-sync-progress",
        json!({"requestId":plan_id,"activeSourceId":source.source_id}),
    );
    let selected = source
        .operations
        .iter()
        .filter(|op| selected_ids.contains(&op.id))
        .collect::<Vec<_>>();
    let has_snapshots = !selected.is_empty();
    let device_record_id = source
        .device_record_id
        .as_ref()
        .filter(|id| selected_ids.contains(*id));
    let repair_id = source
        .index_repair_id
        .as_ref()
        .filter(|id| selected_ids.contains(*id));
    let writer = SourceWriteGuard::acquire(&source.source_id);
    let connection = writer
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|_| cloud::snapshot_remote(&state, &source.source_id));
    let mut expected = source.token.clone();
    if let Some(repair_id) = repair_id.filter(|_| !has_snapshots) {
        let outcome = async {
            cancelled(&plan_id)?;
            if source.error.is_some() || source.upgrade_required {
                return Err("此源未完成预览或尚未启用新协议".into());
            }
            let (remote, root, _, signature) = connection.as_ref().map_err(Clone::clone)?;
            let (events, _) = cancellable_read(&plan_id, remote_events(remote)).await?;
            {
                let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
                session.validate(&repository, root, &events, signature, &expected)?;
            }
            // Repair only the projection of already committed records. Pending
            // and unchecked deletes are never appended here.
            refresh_remote_index_from_events(remote, &events, Some(&plan_id)).await
        }
        .await;
        results.push(OperationResult {
            operation_id: repair_id.clone(),
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
            let (events, _) = cancellable_read(&plan_id, remote_events(remote)).await?;
            {
                let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
                session.validate(&repository, root, &events, signature, &expected)?;
            }
            let committed = execute_operation(
                &state.repository,
                remote,
                root,
                op,
                &events,
                Some(plan_id),
                Some(session),
            )
            .await?;
            let (updated_events, _) = cancellable_read(&plan_id, remote_events(remote)).await?;
            validate_remote_commit(&events, &committed, &updated_events)?;
            let events = updated_events;
            refresh_remote_index_from_events(remote, &events, Some(&plan_id)).await?;
            let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
            session.check_local(&repository)?;
            expected = session.initial.source_token(&events, signature)?;
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
            json!({"requestId":plan_id,"results":[results.last()]}),
        );
    }
    if let Some(repair_id) = repair_id.filter(|_| has_snapshots) {
        let repaired = results
            .iter()
            .filter(|result| result.source_id == source.source_id)
            .all(|result| result.status == "success");
        results.push(OperationResult {
            operation_id: repair_id.clone(),
            source_id: source.source_id.clone(),
            status: if repaired { "success" } else { "failed" }.into(),
            error: (!repaired).then(|| "请重新预览后重试索引修复。".into()),
        });
    }
    let source_succeeded = results
        .iter()
        .filter(|result| result.source_id == source.source_id)
        .all(|result| result.status == "success");
    if device_record_id.is_some() || has_snapshots && source_succeeded {
        let outcome = async {
            if !source_succeeded {
                return Err("部分操作失败，请重新预览后重试设备信息更新。".into());
            }
            if source.error.is_some() || source.upgrade_required {
                return Err("此源未完成预览或尚未启用新协议".into());
            }
            let (remote, root, name, signature) = connection.as_ref().map_err(Clone::clone)?;
            if !has_snapshots {
                let (events, _) = cancellable_read(&plan_id, remote_events(remote)).await?;
                let repository = state.repository.lock().map_err(|_| "资料库状态不可用")?;
                session.validate(&repository, root, &events, signature, &expected)?;
            }
            record_success(
                &state,
                remote,
                root,
                &source.source_id,
                name,
                Some(&plan_id),
            )
            .await
        }
        .await;
        if device_record_id.is_some() || outcome.is_err() {
            results.push(OperationResult {
                operation_id: device_record_id
                    .cloned()
                    .unwrap_or_else(|| "device-record".into()),
                source_id: source.source_id.clone(),
                status: if outcome.is_ok() { "success" } else { "failed" }.into(),
                error: outcome.err(),
            });
        }
    }
    let _ = app.emit(
        "snapshot-sync-progress",
        json!({"requestId":plan_id,"results":results,"finishedSourceId":source.source_id}),
    );
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    thread_local! {
        pub(super) static PROJECTION_ENTRY_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        pub(super) static OBSERVED_SORTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }
    #[test]
    fn revision_merge_sorts_once_per_matching_node_and_keeps_mismatched_history_unobserved() {
        let (_temp, repo, _, snapshot) = fixture();
        let root = repo.lock().unwrap().root().to_owned();
        let identity = device(&repo.lock().unwrap()).unwrap();
        let mut events = (0..640)
            .map(|i| Event {
                operation_id: format!("revision-{i}"),
                device: identity.clone(),
                kind: EventKind::Revised {
                    snapshot: snapshot.clone(),
                    parents: vec!["parent".into(), "parent".into()],
                },
            })
            .collect::<Vec<_>>();
        let mut changed = snapshot.clone();
        changed.note = "remote change".into();
        let mut missing = snapshot.clone();
        missing.id = "missing".into();
        for node in [changed, missing] {
            events.push(Event {
                operation_id: format!("unmatched-{}", events.len()),
                device: identity.clone(),
                kind: EventKind::Revised {
                    snapshot: node,
                    parents: Vec::new(),
                },
            });
        }
        OBSERVED_SORTS.with(|count| count.set(0));
        merge_source_records(&repo, &root, "test", "Cloud", &events).unwrap();
        let observed: HashMap<String, Vec<String>> =
            parse(&fs::read(root.join("config/snapshot-observed-revisions.json")).unwrap())
                .unwrap();
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[&snapshot.id].len(), 641);
        assert_eq!(observed[&snapshot.id][0], "parent");
        assert!(
            !observed[&snapshot.id]
                .iter()
                .any(|id| id.starts_with("unmatched"))
        );
        let known: Vec<Event> =
            parse(&fs::read(root.join("config/snapshot-known-revisions.json")).unwrap()).unwrap();
        assert_eq!(known, events);
        assert_eq!(OBSERVED_SORTS.with(std::cell::Cell::get), 1);
        merge_source_records(&repo, &root, "test", "Cloud", &events).unwrap();
        let retried: HashMap<String, Vec<String>> =
            parse(&fs::read(root.join("config/snapshot-observed-revisions.json")).unwrap())
                .unwrap();
        assert_eq!(retried, observed);
    }
    #[test]
    fn index_projection_visits_entries_linearly_and_preserves_order_and_statistics() {
        let (_temp, repo, _, seed) = fixture();
        let identity = device(&repo.lock().unwrap()).unwrap();
        let events = (0..200)
            .map(|i| {
                let mut snapshot = seed.clone();
                snapshot.id = format!("snapshot-{i}");
                snapshot.entry_id = format!("entry-{i}");
                Event {
                    operation_id: format!("publish-{i}"),
                    device: identity.clone(),
                    kind: EventKind::Published {
                        entry: json!({"id": snapshot.entry_id, "name": i.to_string()}),
                        snapshot,
                        object_path: "sync-v2/objects/test.7z".into(),
                    },
                }
            })
            .collect::<Vec<_>>();
        PROJECTION_ENTRY_READS.with(|count| count.set(0));
        let projected =
            project_remote_index(json!({"entries": [{"id":"empty"}]}), &events).unwrap();
        assert!(PROJECTION_ENTRY_READS.with(std::cell::Cell::get) <= 602);
        let entries = projected["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 201);
        assert_eq!(entries[0]["snapshot_count"], 0);
        assert_eq!(entries[0]["last_snapshot_at_ms"], Value::Null);
        for (i, entry) in entries[1..].iter().enumerate() {
            assert_eq!(entry["id"], format!("entry-{i}"));
            assert_eq!(entry["snapshot_count"], 1);
            assert_eq!(entry["stored_bytes"], seed.size_bytes);
            assert_eq!(entry["last_snapshot_at_ms"], seed.created_at_ms);
            assert_eq!(entry["snapshots"][0]["id"], format!("snapshot-{i}"));
        }
    }
    #[test]
    fn index_projection_retains_legacy_duplicate_and_mismatched_entry_behavior() {
        let (_temp, repo, _, seed) = fixture();
        let mut events = Vec::new();
        for id in ["wrong", "wrong", &seed.entry_id] {
            events.push(Event {
                operation_id: format!("publish-{}", events.len()),
                device: device(&repo.lock().unwrap()).unwrap(),
                kind: EventKind::Published {
                    snapshot: seed.clone(),
                    object_path: "sync-v2/objects/test.7z".into(),
                    entry: json!({"id":id}),
                },
            });
        }
        let projected =
            project_remote_index(json!({"entries": [{"id":"empty"}]}), &events).unwrap();
        let entries = projected["entries"].as_array().unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["empty", "wrong", "wrong", &seed.entry_id]
        );
        assert_eq!(entries[1]["snapshot_count"], 0);
        assert_eq!(entries[3]["snapshots"][0]["id"], seed.id);
    }
    #[tokio::test]
    async fn parallel_recycle_uses_a_committed_tombstone_when_the_other_cloud_copy_is_missing() {
        let (_temp, source, entry, snapshot) = fixture();
        let one = test_remote();
        let two = test_remote();
        transfer(&source, &one, Action::Upload, &snapshot.id)
            .await
            .unwrap();
        transfer(&source, &two, Action::Upload, &snapshot.id)
            .await
            .unwrap();
        let target_temp = tempfile::tempdir().unwrap();
        let target = Arc::new(Mutex::new(
            LocalRepository::open(target_temp.path().join("repo")).unwrap(),
        ));
        transfer(&target, &one, Action::Download, &snapshot.id)
            .await
            .unwrap();
        source
            .lock()
            .unwrap()
            .delete_snapshot(&entry, &snapshot.id)
            .unwrap();
        transfer(&source, &one, Action::RecycleRemote, &snapshot.id)
            .await
            .unwrap();
        transfer(&source, &two, Action::RecycleRemote, &snapshot.id)
            .await
            .unwrap();
        let before = target.lock().unwrap().list_snapshots(&entry).unwrap();
        let events = remote_events(&two).await.unwrap().0;
        let item = plan_sync(&before, &events, &[], false)
            .into_iter()
            .find(|item| item.action == Action::RecycleLocal)
            .unwrap();
        let EventKind::Deleted { object_path, .. } = &item.deletion.as_ref().unwrap().kind else {
            panic!("missing deletion");
        };
        two.delete(object_path).await.unwrap();
        let (root, session) = {
            let repo = target.lock().unwrap();
            (repo.root().to_owned(), SyncSession::new(&repo).unwrap())
        };
        let op = SyncOperation {
            id: "recycle".into(),
            item,
        };
        execute_operation(&target, &one, &root, &op, &events, None, Some(&session))
            .await
            .unwrap();
        assert!(target.lock().unwrap().get_snapshot(&snapshot.id).is_err());
        execute_operation(&target, &two, &root, &op, &events, None, Some(&session))
            .await
            .unwrap();
        assert_eq!(
            target
                .lock()
                .unwrap()
                .list_snapshot_deletions()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn remote_rebase_accepts_only_the_operations_we_published() {
        let identity = Device {
            id: "pc".into(),
            name: "Desktop".into(),
            revision: 1,
        };
        let before = Event {
            operation_id: "before".into(),
            device: identity.clone(),
            kind: EventKind::DeviceNamed,
        };
        let ours = Event {
            operation_id: "ours".into(),
            device: identity.clone(),
            kind: EventKind::DeviceSynced { at_ms: 1 },
        };
        let outside = Event {
            operation_id: "outside".into(),
            device: identity,
            kind: EventKind::DeviceSynced { at_ms: 2 },
        };
        validate_remote_commit(
            &[before.clone()],
            &[ours.clone()],
            &[ours.clone(), before.clone()],
        )
        .unwrap();
        assert!(
            validate_remote_commit(
                &[before.clone()],
                &[ours.clone()],
                &[before.clone(), ours.clone(), outside]
            )
            .is_err()
        );
        assert!(
            validate_remote_commit(&[before.clone()], &[ours.clone()], &[ours.clone()]).is_err()
        );
        let mut rewritten = before.clone();
        rewritten.device.name = "Changed".into();
        assert!(validate_remote_commit(&[before], &[ours.clone()], &[rewritten, ours]).is_err());
    }
    async fn parallel_operations(
        repo: &Arc<Mutex<LocalRepository>>,
        remotes: Vec<SnapshotRemote>,
        action: Action,
    ) -> Vec<Result<(), String>> {
        let (root, local, session) = {
            let repo = repo.lock().unwrap();
            (
                repo.root().to_owned(),
                repo.list_entries()
                    .unwrap()
                    .iter()
                    .flat_map(|entry| repo.list_snapshots(&entry.id).unwrap())
                    .collect::<Vec<_>>(),
                Arc::new(SyncSession::new(&repo).unwrap()),
            )
        };
        let mut workers = tokio::task::JoinSet::new();
        let barrier = Arc::new(tokio::sync::Barrier::new(remotes.len()));
        for remote in remotes {
            let events = remote_events(&remote).await.unwrap().0;
            let token = source_token(&root, &repo.lock().unwrap(), &events, b"test").unwrap();
            let items = plan_sync(&local, &events, &[], false)
                .into_iter()
                .filter(|op| op.action == action)
                .collect::<Vec<_>>();
            assert!(!items.is_empty());
            let (repo, root, session, barrier) =
                (repo.clone(), root.clone(), session.clone(), barrier.clone());
            workers.spawn(async move {
                barrier.wait().await;
                session.validate(&repo.lock().unwrap(), &root, &events, b"test", &token)?;
                for item in items {
                    execute_operation(
                        &repo,
                        &remote,
                        &root,
                        &SyncOperation {
                            id: Uuid::new_v4().to_string(),
                            item,
                        },
                        &events,
                        None,
                        Some(&session),
                    )
                    .await?;
                }
                Ok(())
            });
        }
        let mut results = Vec::new();
        while let Some(result) = workers.join_next().await {
            results.push(result.unwrap());
        }
        results
    }
    #[tokio::test]
    async fn parallel_downloads_merge_distinct_nodes_and_the_same_node_without_foreign_bindings() {
        let (temp, source, entry, first) = fixture();
        fs::write(temp.path().join("save.txt"), "second").unwrap();
        let second = source
            .lock()
            .unwrap()
            .create_snapshot(&entry, "second", "pc", false)
            .unwrap();
        let one = test_remote();
        let two = test_remote();
        transfer(&source, &one, Action::Upload, &first.id)
            .await
            .unwrap();
        transfer(&source, &one, Action::Upload, &second.id)
            .await
            .unwrap();
        transfer(&source, &two, Action::Upload, &first.id)
            .await
            .unwrap();
        let target_temp = tempfile::tempdir().unwrap();
        let target = Arc::new(Mutex::new(
            LocalRepository::open(target_temp.path().join("repo")).unwrap(),
        ));
        for result in parallel_operations(&target, vec![one, two], Action::Download).await {
            result.unwrap();
        }
        let target = target.lock().unwrap();
        assert_eq!(target.list_snapshots(&entry).unwrap().len(), 2);
        assert!(target.list_entries().unwrap()[0].sources.is_empty());
    }
    #[tokio::test]
    async fn parallel_downloads_of_the_same_revision_are_idempotent() {
        let (_temp, source, entry, snapshot) = fixture();
        let one = test_remote();
        let two = test_remote();
        transfer(&source, &one, Action::Upload, &snapshot.id)
            .await
            .unwrap();
        transfer(&source, &two, Action::Upload, &snapshot.id)
            .await
            .unwrap();
        let target_temp = tempfile::tempdir().unwrap();
        let target = Arc::new(Mutex::new(
            LocalRepository::open(target_temp.path().join("repo")).unwrap(),
        ));
        transfer(&target, &one, Action::Download, &snapshot.id)
            .await
            .unwrap();
        source
            .lock()
            .unwrap()
            .update_snapshot_note(&entry, &snapshot.id, "shared revision")
            .unwrap();
        transfer(&source, &one, Action::UploadRevision, &snapshot.id)
            .await
            .unwrap();
        transfer(&source, &two, Action::UploadRevision, &snapshot.id)
            .await
            .unwrap();
        for result in parallel_operations(&target, vec![one, two], Action::DownloadRevision).await {
            result.unwrap();
        }
        assert_eq!(
            target
                .lock()
                .unwrap()
                .get_snapshot(&snapshot.id)
                .unwrap()
                .note,
            "shared revision"
        );
    }
    #[tokio::test]
    async fn parallel_recycling_of_a_revised_node_keeps_one_recovery_and_its_tombstone() {
        let (_temp, source, entry, snapshot) = fixture();
        let one = test_remote();
        let two = test_remote();
        transfer(&source, &one, Action::Upload, &snapshot.id)
            .await
            .unwrap();
        transfer(&source, &two, Action::Upload, &snapshot.id)
            .await
            .unwrap();
        let target_temp = tempfile::tempdir().unwrap();
        let target = Arc::new(Mutex::new(
            LocalRepository::open(target_temp.path().join("repo")).unwrap(),
        ));
        transfer(&target, &one, Action::Download, &snapshot.id)
            .await
            .unwrap();
        {
            let source = source.lock().unwrap();
            source
                .update_snapshot_note(&entry, &snapshot.id, "before recycle")
                .unwrap();
            source.delete_snapshot(&entry, &snapshot.id).unwrap();
        }
        transfer(&source, &one, Action::RecycleRemote, &snapshot.id)
            .await
            .unwrap();
        transfer(&source, &two, Action::RecycleRemote, &snapshot.id)
            .await
            .unwrap();
        for result in parallel_operations(&target, vec![one, two], Action::RecycleLocal).await {
            result.unwrap();
        }
        let target = target.lock().unwrap();
        assert!(target.list_snapshots(&entry).unwrap().is_empty());
        let deleted = target.list_snapshot_deletions().unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].snapshot.note, "before recycle");
        assert!(
            target
                .root()
                .join("recycle-snapshots")
                .join(format!("{}.7z", deleted[0].operation_id))
                .exists()
        );
    }
    #[test]
    fn concurrent_cloud_record_merges_keep_every_sources_revision_history() {
        let (_temp, repo, _, snapshot) = fixture();
        let root = repo.lock().unwrap().root().to_owned();
        let identity = device(&repo.lock().unwrap()).unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let workers = (0..8)
            .map(|index| {
                let (repo, root, identity, snapshot, barrier) = (
                    repo.clone(),
                    root.clone(),
                    identity.clone(),
                    snapshot.clone(),
                    barrier.clone(),
                );
                std::thread::spawn(move || {
                    let events = (0..80)
                        .map(|revision| Event {
                            operation_id: format!("source-{index}-revision-{revision}"),
                            device: identity.clone(),
                            kind: EventKind::Revised {
                                snapshot: snapshot.clone(),
                                parents: Vec::new(),
                            },
                        })
                        .collect::<Vec<_>>();
                    barrier.wait();
                    merge_source_records(&repo, &root, &index.to_string(), "Cloud", &events)
                        .unwrap();
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker.join().unwrap();
        }
        let known: Vec<Event> =
            parse(&fs::read(root.join("config/snapshot-known-revisions.json")).unwrap()).unwrap();
        assert_eq!(
            known.len(),
            640,
            "concurrent clouds overwrote each others history"
        );
        let observed: HashMap<String, Vec<String>> =
            parse(&fs::read(root.join("config/snapshot-observed-revisions.json")).unwrap())
                .unwrap();
        assert_eq!(observed[&snapshot.id].len(), 640);
    }
    #[test]
    fn parallel_session_accepts_its_own_commit_but_rejects_external_edits() {
        let (_temp, repo, entry, snapshot) = fixture();
        let repo = repo.lock().unwrap();
        let token = source_token(repo.root(), &repo, &[], b"source").unwrap();
        let session = SyncSession::new(&repo).unwrap();
        session
            .commit(&repo, |repo| {
                repo.update_snapshot_note(&entry, &snapshot.id, "sync revision")
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        session
            .validate(&repo, repo.root(), &[], b"source", &token)
            .unwrap();
        repo.set_snapshot_locked(&entry, &snapshot.id, true)
            .unwrap();
        assert!(
            session
                .validate(&repo, repo.root(), &[], b"source", &token)
                .is_err()
        );
    }
    #[tokio::test]
    async fn all_selected_sources_start_together_and_empty_sources_never_start() {
        let sources = (0..5)
            .map(|index| SourcePlan {
                source_id: index.to_string(),
                source_name: index.to_string(),
                upgrade_required: false,
                error: None,
                token: String::new(),
                operations: Vec::new(),
                index_repair_id: (index < 4).then(|| format!("repair-{index}")),
                device_record_id: None,
                archive_names: HashMap::new(),
                devices: Vec::new(),
            })
            .collect();
        let selected = (0..4).map(|index| format!("repair-{index}")).collect();
        let barrier = Arc::new(tokio::sync::Barrier::new(4));
        let run = run_selected_sources(sources, &selected, move |source| {
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
                vec![OperationResult {
                    operation_id: format!("repair-{}", source.source_id),
                    status: if source.source_id == "1" {
                        "failed"
                    } else {
                        "success"
                    }
                    .into(),
                    error: None,
                    source_id: source.source_id,
                }]
            }
        });
        let result = tokio::time::timeout(std::time::Duration::from_millis(250), run).await;
        assert!(
            result.is_ok(),
            "clouds still start serially or have a global concurrency cap"
        );
        let results = result.unwrap().unwrap();
        assert_eq!(results.len(), 4);
        assert_eq!(
            results
                .iter()
                .filter(|item| item.status == "success")
                .count(),
            3
        );
    }
    #[test]
    fn unchanged_device_name_does_not_invent_work_but_rename_still_syncs_without_new_snapshots() {
        let identity = Device {
            id: "pc".into(),
            name: "Desktop".into(),
            revision: 1,
        };
        assert!(device_record_needed(&identity, &[]));
        let events = vec![Event {
            operation_id: "device-pc-1".into(),
            device: identity.clone(),
            kind: EventKind::DeviceNamed,
        }];
        assert!(!device_record_needed(&identity, &events));
        assert!(device_record_needed(
            &Device {
                name: "Renamed".into(),
                revision: 2,
                ..identity
            },
            &events
        ));
    }
    #[tokio::test]
    async fn preview_detects_index_drift_without_repairing_or_inventing_empty_work() {
        let (_temp, repo, _, snapshot) = fixture();
        let remote = test_remote();
        assert!(!index_repair_needed(&remote, &[], None).await.unwrap());
        assert!(
            remote
                .optional_bytes("catalog.json")
                .await
                .unwrap()
                .is_none()
        );
        transfer(&repo, &remote, Action::Upload, &snapshot.id)
            .await
            .unwrap();
        let events = remote_events(&remote).await.unwrap().0;
        assert!(!index_repair_needed(&remote, &events, None).await.unwrap());
        remote
            .put(
                "catalog.json",
                serialize(
                    &json!({"format_version":4,"updated_at_ms":0,"categories":[],"entries":[]}),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert!(index_repair_needed(&remote, &events, None).await.unwrap());
        let catalog: Value = parse(
            &remote
                .optional_bytes("catalog.json")
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert!(
            catalog["entries"].as_array().unwrap().is_empty(),
            "preview repaired the remote"
        );
        refresh_remote_index_from_events(&remote, &events, None)
            .await
            .unwrap();
        assert!(!index_repair_needed(&remote, &events, None).await.unwrap());
    }
    #[test]
    fn execution_skips_sources_without_selected_work_including_unchecked_repairs() {
        let source = SourcePlan {
            source_id: "test".into(),
            source_name: "Test".into(),
            upgrade_required: false,
            error: None,
            token: String::new(),
            operations: Vec::new(),
            index_repair_id: Some("repair".into()),
            device_record_id: None,
            archive_names: HashMap::new(),
            devices: Vec::new(),
        };
        assert!(!source_has_selected_work(&source, &BTreeSet::new()));
        assert!(source_has_selected_work(
            &source,
            &BTreeSet::from(["repair".into()])
        ));
        let empty = SourcePlan {
            index_repair_id: None,
            ..source
        };
        assert!(!source_has_selected_work(
            &empty,
            &BTreeSet::from(["repair".into()])
        ));
    }
    #[test]
    fn upload_archive_read_does_not_hold_the_repository_mutex() {
        let (_temp, repo, _, snapshot) = fixture();
        let root = repo.lock().unwrap().root().to_owned();
        let (bytes, _, _) = prepare_upload_payload(&repo, &root, &snapshot, |path| {
            assert!(
                repo.try_lock().is_ok(),
                "archive read blocks metadata access"
            );
            fs::read(path)
        })
        .unwrap();
        assert_eq!(digest(&bytes), snapshot.object_hash);
    }
    #[tokio::test]
    async fn cancelled_upload_does_not_publish_an_object_or_event() {
        let (_temp, repo, _, snapshot) = fixture();
        let root = repo.lock().unwrap().root().to_owned();
        let remote = test_remote();
        let item = plan_sync(&[snapshot.clone()], &[], &[], false).remove(0);
        let id = "cancelled-upload-test";
        begin(id).unwrap();
        cancel_snapshot_sync(id.into()).unwrap();
        let result = execute_operation(
            &repo,
            &remote,
            &root,
            &SyncOperation {
                id: "op".into(),
                item,
            },
            &[],
            Some(id),
            None,
        )
        .await;
        end(id);
        assert!(result.is_err());
        assert!(remote.load_events().await.unwrap().is_empty());
        assert!(
            remote
                .optional_bytes(&format!(
                    "sync-v2/objects/{}-{}.7z",
                    snapshot.id, snapshot.object_hash
                ))
                .await
                .unwrap()
                .is_none()
        );
    }
    #[tokio::test]
    async fn same_id_rewrites_and_missing_events_invalidate_the_previous_token() {
        let (_temp, repo, _, _) = fixture();
        let remote = test_remote();
        let mut event = Event {
            operation_id: "mutable-record".into(),
            device: Device {
                id: "device".into(),
                name: "original".into(),
                revision: 0,
            },
            kind: EventKind::DeviceNamed,
        };
        remote.append_event(&event).await.unwrap();
        let before = remote_events(&remote).await.unwrap().0;
        let previous_token = {
            let repo = repo.lock().unwrap();
            source_token(repo.root(), &repo, &before, b"config").unwrap()
        };
        event.device.name = "changed under the same ID".into();
        remote
            .put(
                "sync-v2/operations/mutable-record.json",
                serialize(&event).unwrap(),
            )
            .await
            .unwrap();
        let changed = remote_events(&remote).await.unwrap().0;
        {
            let repo = repo.lock().unwrap();
            assert_ne!(
                source_token(repo.root(), &repo, &changed, b"config").unwrap(),
                previous_token
            );
        }
        remote
            .delete("sync-v2/operations/mutable-record.json")
            .await
            .unwrap();
        let missing = remote_events(&remote).await.unwrap().0;
        let repo = repo.lock().unwrap();
        assert_ne!(
            source_token(repo.root(), &repo, &missing, b"config").unwrap(),
            previous_token
        );
    }
    #[tokio::test]
    async fn index_projection_does_not_rescan_verified_events() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            while !server_stop.load(Ordering::Relaxed) {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    continue;
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = [0; 8192];
                let count = socket.read(&mut bytes).unwrap();
                let request = String::from_utf8_lossy(&bytes[..count]);
                let first = request.lines().next().unwrap().to_owned();
                let status = if first.starts_with("GET") || first.starts_with("PROPFIND") {
                    "404 Not Found"
                } else {
                    "201 Created"
                };
                requests.push(first);
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            }
            requests
        });
        let client = chronicle_sync::WebDavClient::new(
            chronicle_sync::WebDavSource {
                endpoint: format!("http://{address}"),
                username: String::new(),
                password: String::new(),
                remote_path: "Chronicle".into(),
            },
            chronicle_sync::RequestPolicy {
                request_delay_ms: 0,
                retry_limit: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let result = refresh_remote_index_from_events(
            &SnapshotRemote::Store(chronicle_sync::RemoteStore::LegacyWebDav(client)),
            &[],
            None,
        )
        .await;
        stop.store(true, Ordering::Relaxed);
        let requests = server.join().unwrap();
        result.unwrap();
        assert_eq!(
            requests.len(),
            3,
            "only catalog GET, temporary PUT, atomic MOVE: {requests:?}"
        );
        assert!(!requests.iter().any(|request| request.contains("sync-v2")));
    }
    #[tokio::test]
    async fn cancellation_wakes_every_concurrent_source_read() {
        let id = "parallel-read-cancel";
        begin(id).unwrap();
        let mut workers = tokio::task::JoinSet::new();
        let (started, mut ready) = tokio::sync::mpsc::unbounded_channel();
        for _ in 0..4 {
            let started = started.clone();
            workers.spawn(async move {
                cancellable_read(id, async {
                    started.send(()).unwrap();
                    std::future::pending::<Result<(), String>>().await
                })
                .await
            });
        }
        for _ in 0..4 {
            ready.recv().await.unwrap();
        }
        cancel_snapshot_sync(id.into()).unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_millis(250), async {
            while let Some(result) = workers.join_next().await {
                assert!(result.unwrap().is_err());
            }
        })
        .await;
        end(id);
        assert!(result.is_ok(), "cancel woke only the last cloud reader");
    }
    #[tokio::test]
    async fn cancellation_drops_a_pending_read_without_waiting_for_network_timeout() {
        let id = "pending-read-cancel";
        begin(id).unwrap();
        let dropped = Arc::new(AtomicBool::new(false));
        struct PendingGuard(Arc<AtomicBool>);
        impl Drop for PendingGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let (started, ready) = tokio::sync::oneshot::channel();
        let read_dropped = dropped.clone();
        let worker = tokio::spawn(async move {
            cancellable_read(id, async {
                let _guard = PendingGuard(read_dropped);
                started.send(()).unwrap();
                std::future::pending::<Result<(), String>>().await
            })
            .await
        });
        ready.await.unwrap();
        cancel_snapshot_sync(id.into()).unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_millis(250), worker).await;
        end(id);
        assert!(result.is_ok(), "cancel left the network future pending");
        assert!(result.unwrap().unwrap().is_err());
        assert!(dropped.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn cancellation_before_registration_does_not_start_the_read() {
        let id = "cancel-before-registration";
        cancel_snapshot_sync(id.into()).unwrap();
        begin(id).unwrap();
        let mut polled = false;
        let result = cancellable_read(id, async {
            polled = true;
            Ok(())
        })
        .await;
        end(id);
        assert!(result.is_err());
        assert!(!polled);
    }
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
            None,
            None,
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
            execute_operation(&repo, &remote, &root, &upload, &[], None, None)
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
            execute_operation(
                &other,
                &remote,
                other_temp.path(),
                &download,
                &stale,
                None,
                None
            )
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
