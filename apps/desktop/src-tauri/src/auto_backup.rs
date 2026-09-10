use std::{collections::HashMap, path::PathBuf, sync::{Arc, Mutex}, thread, time::Duration};

use chronicle_core::Entry;
use chronicle_storage::LocalRepository;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::AppState;

pub struct AutoBackupManager {
    repository: Arc<Mutex<LocalRepository>>,
    app: AppHandle,
    watchers: Mutex<Vec<RecommendedWatcher>>,
    pending: Arc<Mutex<HashMap<String, u64>>>,
}

impl AutoBackupManager {
    pub fn new(repository: Arc<Mutex<LocalRepository>>, app: AppHandle) -> Self {
        Self { repository, app, watchers: Mutex::new(Vec::new()), pending: Arc::new(Mutex::new(HashMap::new())) }
    }

    pub fn refresh(&self) -> Result<(), String> {
        self.watchers.lock().map_err(|_| "自动备份监听器不可用".to_owned())?.clear();
        self.pending.lock().map_err(|_| "自动备份监听器不可用".to_owned())?.clear();
        let (delay, retention, entries) = {
            let repository = self.repository.lock().map_err(|_| "Chronicle 本地仓库状态不可用".to_owned())?;
            let settings = repository.load_settings().map_err(|error| error.to_string())?;
            let app = settings.get("app").cloned().unwrap_or(Value::Null);
            let delay = app.get("autoBackupDelaySeconds").and_then(Value::as_u64).unwrap_or(5).clamp(1, 300);
            let retention = app.get("retentionCount").and_then(Value::as_u64).map(|value| value as usize);
            let entries = repository.list_entries().map_err(|error| error.to_string())?;
            (delay, retention, entries)
        };
        let entries: Vec<Entry> = entries.into_iter().filter(|entry| entry.auto_backup_enabled && entry.sources.iter().any(|source| !source.path.is_empty())).collect();
        for entry in entries.clone() {
            for source in &entry.sources {
                let path = PathBuf::from(&source.path);
                if !path.exists() { continue; }
                let roots = entries.clone();
                let repository = self.repository.clone();
                let pending = self.pending.clone();
                let app = self.app.clone();
                let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    let Ok(event) = event else { return; };
                    for entry in roots.iter().filter(|entry| entry.sources.iter().any(|source| {
                        let source_path = PathBuf::from(&source.path);
                        event.paths.iter().any(|changed| changed.starts_with(&source_path) || (source_path.is_file() && changed == &source_path))
                    })) {
                        let token = {
                            let mut pending = pending.lock().expect("auto backup pending lock");
                            let token = pending.get(&entry.id).copied().unwrap_or(0) + 1;
                            pending.insert(entry.id.clone(), token);
                            token
                        };
                        let repository = repository.clone(); let pending = pending.clone(); let app = app.clone(); let entry_id = entry.id.clone();
                        thread::spawn(move || {
                            thread::sleep(Duration::from_secs(delay));
                            if pending.lock().ok().and_then(|values| values.get(&entry_id).copied()) != Some(token) { return; }
                            let result = (|| -> Result<(), String> {
                                let repository = repository.lock().map_err(|_| "Chronicle 本地仓库状态不可用".to_owned())?;
                                let (device, _) = repository.device_identity().map_err(|error| error.to_string())?;
                                repository.create_snapshot(&entry_id, "自动备份", device, false).map_err(|error| error.to_string())?;
                                if let Some(limit) = retention {
                                    let snapshots = repository.list_snapshots(&entry_id).map_err(|error| error.to_string())?;
                                    let ordinary: Vec<_> = snapshots.iter().filter(|snapshot| !snapshot.safety).collect();
                                    let excess = ordinary.len().saturating_sub(limit);
                                    for snapshot in ordinary.into_iter().take(excess) { repository.delete_snapshot(&entry_id, &snapshot.id).map_err(|error| error.to_string())?; }
                                }
                                Ok(())
                            })();
                            let event = if result.is_ok() { "auto-backup-created" } else { "auto-backup-failed" };
                            let _ = app.emit(event, serde_json::json!({ "archiveId": entry_id, "error": result.err() }));
                        });
                    }
                }).map_err(|error| error.to_string())?;
                watcher.watch(&path, if path.is_dir() { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive }).map_err(|error| error.to_string())?;
                self.watchers.lock().map_err(|_| "自动备份监听器不可用".to_owned())?.push(watcher);
            }
        }
        Ok(())
    }
}

#[tauri::command(async)]
pub fn refresh_auto_backup(state: State<'_, AppState>) -> Result<(), String> { state.auto_backup.refresh() }
