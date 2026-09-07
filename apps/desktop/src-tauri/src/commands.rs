use chronicle_core::{ChangeSummary, Entry, EntryKind, Snapshot, SnapshotFile};
use serde::Serialize;
use tauri::State;

use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDto {
    id: String,
    name: String,
    source_path: String,
    category: String,
    kind: &'static str,
    created_at: u64,
    updated_at: u64,
    total_bytes: u64,
    last_snapshot_at: Option<u64>,
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

fn entry_dto(entry: Entry, latest: Option<&Snapshot>) -> EntryDto {
    EntryDto {
        id: entry.id,
        name: entry.name,
        source_path: entry.source_path,
        category: entry.category_id.unwrap_or_else(|| "未分类".into()),
        kind: match entry.kind {
            EntryKind::File => "file",
            EntryKind::Directory => "folder",
        },
        created_at: entry.created_at_ms,
        updated_at: latest.map_or(entry.created_at_ms, |snapshot| snapshot.created_at_ms),
        total_bytes: latest.map_or(0, |snapshot| {
            snapshot.files.iter().map(|file| file.size_bytes).sum()
        }),
        last_snapshot_at: latest.map(|snapshot| snapshot.created_at_ms),
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
            Ok(entry_dto(entry, latest.as_ref()))
        })
        .collect()
}

#[tauri::command(async)]
pub fn add_entry(
    state: State<'_, AppState>,
    source_path: String,
    category_id: Option<String>,
) -> Result<EntryDto, String> {
    let repository = state.repository.lock().map_err(|_| state_error())?;
    repository
        .add_entry(source_path, None, category_id)
        .map(|entry| entry_dto(entry, None))
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
