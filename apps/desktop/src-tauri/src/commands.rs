use chronicle_core::{
    Category, ChangeSummary, Entry, EntryKind, EntrySource, Snapshot, SnapshotFile, StoragePolicy,
};
use serde::Serialize;
use serde_json::Value;
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
) -> Result<EntryDto, String> {
    let storage_policy = match storage_policy.as_str() {
        "local" => StoragePolicy::Local,
        "local_and_remote" => StoragePolicy::LocalAndRemote,
        _ => return Err("不支持的保存方式".into()),
    };
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .add_entry_sources(&name, &source_paths, category_id, storage_policy)
        .map(|entry| entry_dto(entry, None, None))
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn load_settings(state: State<'_, AppState>) -> Result<Value, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository.load_settings().map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn save_settings(state: State<'_, AppState>, settings: Value) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .save_settings(&settings)
        .map_err(|error| error.to_string())
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
    repository
        .create_snapshot(&entry_id, title, "local", safety)
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
