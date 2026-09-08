#![allow(clippy::needless_pass_by_value)]

use chronicle_core::{
    Category, ChangeSummary, Entry, EntryKind, EntrySource, Snapshot, SnapshotFile, StoragePolicy,
    SyncMode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use tauri::State;

use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDto {
    id: String,
    name: String,
    source_path: String,
    sources: Vec<EntrySourceDto>,
    category: String,
    category_id: Option<String>,
    tags: Vec<String>,
    kind: &'static str,
    storage_policy: &'static str,
    sync_mode: &'static str,
    created_at: u64,
    updated_at: u64,
    total_bytes: u64,
    last_snapshot_at: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    id: String,
    name: String,
    parent_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryInfoDto {
    path: String,
    total_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleItemDto {
    id: String,
    kind: String,
    display_name: String,
    deleted_at: u64,
    size_bytes: u64,
    entry_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySourceDto {
    id: String,
    name: String,
    path: String,
    kind: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFileDto {
    path: String,
    size: u64,
    last_modified: u64,
    hash: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDto {
    id: String,
    archive_id: String,
    title: String,
    created_at: u64,
    total_bytes: u64,
    content_hash: String,
    files: Vec<SnapshotFileDto>,
    changes: ChangeSummary,
    safety: bool,
    device_id: String,
    device_name: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEntryDto {
    id: String,
    occurred_at: u64,
    operation: String,
    #[serde(default)]
    archive_id: Option<String>,
    #[serde(default)]
    source_id: Option<String>,
    message: String,
    details: String,
}

#[derive(Deserialize, Serialize)]
struct DiagnosticsDocument {
    #[serde(rename = "formatVersion")]
    format_version: u8,
    entries: Vec<DiagnosticEntryDto>,
}

fn entry_dto(entry: Entry, latest: Option<&Snapshot>, category_name: Option<String>) -> EntryDto {
    let source_path = entry
        .sources
        .iter()
        .map(|source| source.path.as_str())
        .collect::<Vec<_>>()
        .join(" · ");
    let kind = if entry.sources.len() == 1 {
        match entry.sources[0].kind {
            EntryKind::File => "file",
            EntryKind::Directory => "folder",
        }
    } else {
        "collection"
    };
    EntryDto {
        id: entry.id,
        name: entry.name,
        source_path,
        sources: entry.sources.into_iter().map(entry_source_dto).collect(),
        category: category_name.unwrap_or_else(|| "未分类".into()),
        category_id: entry.category_id,
        tags: entry.tags,
        kind,
        storage_policy: match entry.storage_policy {
            StoragePolicy::Local => "local",
            StoragePolicy::LocalAndRemote => "local_and_remote",
        },
        sync_mode: match entry.sync_mode {
            SyncMode::Manual => "manual",
            SyncMode::Automatic => "automatic",
        },
        created_at: entry.created_at_ms,
        updated_at: latest.map_or(entry.created_at_ms, |snapshot| snapshot.created_at_ms),
        total_bytes: latest.map_or(0, |snapshot| {
            snapshot.files.iter().map(|file| file.size_bytes).sum()
        }),
        last_snapshot_at: latest.map(|snapshot| snapshot.created_at_ms),
    }
}

fn category_dto(category: Category) -> CategoryDto {
    CategoryDto {
        id: category.id,
        name: category.name,
        parent_id: category.parent_id,
    }
}

fn entry_source_dto(source: EntrySource) -> EntrySourceDto {
    EntrySourceDto {
        id: source.id,
        name: source.name,
        path: source.path,
        kind: match source.kind {
            EntryKind::File => "file",
            EntryKind::Directory => "folder",
        },
    }
}

#[tauri::command(async)]
pub fn repository_info(state: State<'_, AppState>) -> Result<RepositoryInfoDto, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let recycle = settings_recycle_root(&repository);
    Ok(RepositoryInfoDto {
        path: repository.root().to_string_lossy().into_owned(),
        total_bytes: repository
            .total_stored_bytes_with_recycle(recycle.as_deref())
            .map_err(|error| error.to_string())?,
    })
}

fn settings_recycle_root(repository: &chronicle_storage::LocalRepository) -> Option<PathBuf> {
    let settings = repository.load_settings().ok()?;
    let enabled = settings
        .pointer("/app/recycleBinEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    recycle_root(
        repository,
        enabled,
        settings
            .pointer("/app/recycleBinPath")
            .and_then(Value::as_str)
            .map(str::to_owned),
    )
}

#[tauri::command(async)]
pub fn list_recycle_items(state: State<'_, AppState>) -> Result<Vec<RecycleItemDto>, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let root =
        settings_recycle_root(&repository).unwrap_or_else(|| repository.root().join("recycle"));
    repository
        .list_recycle_items(&root)
        .map(|items| {
            items
                .into_iter()
                .map(|item| RecycleItemDto {
                    id: item.id,
                    kind: item.kind,
                    display_name: item.display_name,
                    deleted_at: item.deleted_at_ms,
                    size_bytes: item.size_bytes,
                    entry_count: item.entry_count,
                })
                .collect()
        })
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn restore_recycle_item(state: State<'_, AppState>, item_id: String) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let root =
        settings_recycle_root(&repository).unwrap_or_else(|| repository.root().join("recycle"));
    repository
        .restore_recycle_item(&root, &item_id)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn permanently_delete_recycle_item(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let root =
        settings_recycle_root(&repository).unwrap_or_else(|| repository.root().join("recycle"));
    repository
        .permanently_delete_recycle_item(&root, &item_id)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn empty_recycle_bin(state: State<'_, AppState>) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let root =
        settings_recycle_root(&repository).unwrap_or_else(|| repository.root().join("recycle"));
    repository
        .empty_recycle_bin(&root)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn open_repository_folder(state: State<'_, AppState>) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let path = repository.root().to_path_buf();
    drop(repository);
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    command
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn open_recycle_bin(
    state: State<'_, AppState>,
    recycle_bin_path: Option<String>,
) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let path = recycle_bin_path
        .filter(|value| !value.trim().is_empty())
        .map_or_else(|| repository.root().join("recycle"), PathBuf::from);
    drop(repository);
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    command
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn snapshot_file_dto(file: SnapshotFile) -> SnapshotFileDto {
    SnapshotFileDto {
        path: file.relative_path,
        size: file.size_bytes,
        last_modified: file.modified_at_ms,
        hash: file.content_hash,
    }
}

fn snapshot_dto(snapshot: Snapshot) -> SnapshotDto {
    SnapshotDto {
        id: snapshot.id,
        archive_id: snapshot.entry_id,
        title: snapshot.title,
        created_at: snapshot.created_at_ms,
        total_bytes: snapshot.files.iter().map(|file| file.size_bytes).sum(),
        content_hash: snapshot.object_hash,
        files: snapshot.files.into_iter().map(snapshot_file_dto).collect(),
        changes: snapshot.changes,
        safety: snapshot.safety,
        device_id: snapshot.device_id,
        device_name: snapshot.device_name,
    }
}

fn state_error() -> String {
    "Chronicle 本地仓库状态不可用".into()
}

#[tauri::command(async)]
pub fn list_entries(state: State<'_, AppState>) -> Result<Vec<EntryDto>, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let categories = repository
        .list_categories()
        .map_err(|error| error.to_string())?;
    repository
        .list_entries()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|entry| {
            let latest = repository
                .list_snapshots(&entry.id)
                .map_err(|error| error.to_string())?
                .into_iter()
                .next();
            let category_name = entry.category_id.as_ref().and_then(|category_id| {
                categories
                    .iter()
                    .find(|category| &category.id == category_id)
                    .map(|category| category.name.clone())
            });
            Ok(entry_dto(entry, latest.as_ref(), category_name))
        })
        .collect()
}

#[tauri::command(async)]
pub fn add_entry(
    state: State<'_, AppState>,
    name: String,
    source_paths: Vec<String>,
    category_id: Option<String>,
    storage_policy: String,
    sync_mode: String,
) -> Result<EntryDto, String> {
    let storage_policy = match storage_policy.as_str() {
        "local" => StoragePolicy::Local,
        "local_and_remote" => StoragePolicy::LocalAndRemote,
        _ => return Err("不支持的保存方式".into()),
    };
    let sync_mode = parse_sync_mode(&sync_mode)?;
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .add_entry_sources_with_sync(&name, &source_paths, category_id, storage_policy, sync_mode)
        .map(|entry| entry_dto(entry, None, None))
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn update_entry(
    state: State<'_, AppState>,
    entry_id: String,
    name: String,
    source_paths: Vec<String>,
    storage_policy: String,
    sync_mode: String,
) -> Result<EntryDto, String> {
    let storage_policy = match storage_policy.as_str() {
        "local" => StoragePolicy::Local,
        "local_and_remote" => StoragePolicy::LocalAndRemote,
        _ => return Err("不支持的保存方式".into()),
    };
    let sync_mode = parse_sync_mode(&sync_mode)?;
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .update_entry_with_sync(&entry_id, &name, &source_paths, storage_policy, sync_mode)
        .map(|entry| entry_dto(entry, None, None))
        .map_err(|error| error.to_string())
}

fn parse_sync_mode(value: &str) -> Result<SyncMode, String> {
    match value {
        "manual" => Ok(SyncMode::Manual),
        "automatic" => Ok(SyncMode::Automatic),
        _ => Err("不支持的同步方式".into()),
    }
}

fn recycle_root(
    repository: &chronicle_storage::LocalRepository,
    enabled: bool,
    configured_path: Option<String>,
) -> Option<PathBuf> {
    enabled.then(|| {
        configured_path
            .filter(|path| !path.trim().is_empty())
            .map_or_else(|| repository.root().join("recycle"), PathBuf::from)
    })
}

#[tauri::command(async)]
pub fn delete_entry(
    state: State<'_, AppState>,
    entry_id: String,
    recycle_bin_enabled: bool,
    recycle_bin_path: Option<String>,
) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let recycle = recycle_root(&repository, recycle_bin_enabled, recycle_bin_path);
    repository
        .delete_entry(&entry_id, recycle.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn delete_category(
    state: State<'_, AppState>,
    category_id: String,
    recycle_bin_enabled: bool,
    recycle_bin_path: Option<String>,
) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let recycle = recycle_root(&repository, recycle_bin_enabled, recycle_bin_path);
    repository
        .delete_category(&category_id, recycle.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn load_settings(state: State<'_, AppState>) -> Result<Value, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .load_settings()
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn save_settings(state: State<'_, AppState>, settings: Value) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .save_settings(&settings)
        .map_err(|error| error.to_string())
}

fn diagnostics_path(repository: &chronicle_storage::LocalRepository) -> PathBuf {
    repository.root().join("config").join("diagnostics.json")
}

fn read_diagnostics(path: &std::path::Path) -> Result<DiagnosticsDocument, String> {
    if !path.is_file() {
        return Ok(DiagnosticsDocument {
            format_version: 1,
            entries: Vec::new(),
        });
    }
    serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn write_diagnostics(path: &std::path::Path, document: &DiagnosticsDocument) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "错误记录路径无效".to_owned())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(".diagnostics-{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(document).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    std::fs::rename(temporary, path).map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn append_diagnostic(
    state: State<'_, AppState>,
    entry: DiagnosticEntryDto,
) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let path = diagnostics_path(&repository);
    drop(repository);
    let mut document = read_diagnostics(&path)?;
    document.entries.insert(0, entry);
    document.entries.truncate(200);
    write_diagnostics(&path, &document)
}

#[tauri::command(async)]
pub fn list_diagnostics(state: State<'_, AppState>) -> Result<Vec<DiagnosticEntryDto>, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let path = diagnostics_path(&repository);
    drop(repository);
    Ok(read_diagnostics(&path)?.entries)
}

#[tauri::command(async)]
pub fn clear_diagnostics(state: State<'_, AppState>) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let path = diagnostics_path(&repository);
    drop(repository);
    write_diagnostics(
        &path,
        &DiagnosticsDocument {
            format_version: 1,
            entries: Vec::new(),
        },
    )
}

#[tauri::command(async)]
pub fn set_entry_category(
    state: State<'_, AppState>,
    entry_id: String,
    category_id: Option<String>,
) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .set_entry_category(&entry_id, category_id)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn set_entry_tags(
    state: State<'_, AppState>,
    entry_id: String,
    tags: Vec<String>,
) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .set_entry_tags(&entry_id, tags)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn list_categories(state: State<'_, AppState>) -> Result<Vec<CategoryDto>, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .list_categories()
        .map(|categories| categories.into_iter().map(category_dto).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn create_category(
    state: State<'_, AppState>,
    name: String,
    parent_id: Option<String>,
) -> Result<CategoryDto, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .create_category(&name, parent_id)
        .map(category_dto)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn move_category(
    state: State<'_, AppState>,
    category_id: String,
    parent_id: Option<String>,
) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .move_category(&category_id, parent_id.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn list_snapshots(
    state: State<'_, AppState>,
    entry_id: String,
) -> Result<Vec<SnapshotDto>, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .list_snapshots(&entry_id)
        .map(|snapshots| snapshots.into_iter().map(snapshot_dto).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn create_snapshot(
    state: State<'_, AppState>,
    entry_id: String,
    title: String,
    safety: bool,
) -> Result<SnapshotDto, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    let (device_id, _) = repository
        .device_identity()
        .map_err(|error| error.to_string())?;
    repository
        .create_snapshot(&entry_id, title, device_id, safety)
        .map(snapshot_dto)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn verify_snapshot(state: State<'_, AppState>, snapshot_id: String) -> Result<bool, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .verify_snapshot(&snapshot_id)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn restore_snapshot(
    state: State<'_, AppState>,
    entry_id: String,
    snapshot_id: String,
) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .restore_snapshot(&entry_id, &snapshot_id)
        .map_err(|error| error.to_string())
}
