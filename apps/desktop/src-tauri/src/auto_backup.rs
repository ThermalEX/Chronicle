use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

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
    pending: Arc<Mutex<HashMap<String, bool>>>,
    restore_suppressions: Arc<Mutex<HashMap<String, Option<Instant>>>>,
}

/// Returns whether this is the first change in an archive's active coalescing window.
/// Later changes are remembered so the window can create one final, latest snapshot.
fn record_auto_backup_change(windows: &mut HashMap<String, bool>, entry_id: &str) -> bool {
    if let Some(has_follow_up_change) = windows.get_mut(entry_id) {
        *has_follow_up_change = true;
        false
    } else {
        windows.insert(entry_id.to_owned(), false);
        true
    }
}

fn take_trailing_backup(windows: &mut HashMap<String, bool>, entry_id: &str) -> bool {
    windows.remove(entry_id).unwrap_or(false)
}

/// Blocks watcher events while a restore is writing, then for one merge window afterwards.
/// `None` represents an in-progress restore whose completion time is not known yet.
fn begin_restore_suppression(
    suppressions: &mut HashMap<String, Option<Instant>>,
    entry_id: &str,
) {
    suppressions.insert(entry_id.to_owned(), None);
}

fn finish_restore_suppression(
    suppressions: &mut HashMap<String, Option<Instant>>,
    entry_id: &str,
    now: Instant,
    delay: Duration,
) {
    suppressions.insert(entry_id.to_owned(), Some(now + delay));
}

fn cancel_restore_suppression(
    suppressions: &mut HashMap<String, Option<Instant>>,
    entry_id: &str,
) {
    suppressions.remove(entry_id);
}

fn is_restore_suppressed(
    suppressions: &mut HashMap<String, Option<Instant>>,
    entry_id: &str,
    now: Instant,
) -> bool {
    match suppressions.get(entry_id).copied() {
        Some(None) => true,
        Some(Some(until)) if now < until => true,
        Some(Some(_)) => {
            suppressions.remove(entry_id);
            false
        }
        None => false,
    }
}

fn create_auto_backup_snapshot(
    repository: &Arc<Mutex<LocalRepository>>,
    entry_id: &str,
    retention: Option<usize>,
) -> Result<(), String> {
    let repository = repository
        .lock()
        .map_err(|_| "Chronicle 本地仓库状态不可用".to_owned())?;
    let (device, _) = repository
        .device_identity()
        .map_err(|error| error.to_string())?;
    repository
        .create_snapshot(entry_id, "自动备份", device, false)
        .map_err(|error| error.to_string())?;
    if let Some(limit) = retention {
        let snapshots = repository
            .list_snapshots(entry_id)
            .map_err(|error| error.to_string())?;
        let ordinary: Vec<_> = snapshots
            .iter()
            .filter(|snapshot| !snapshot.safety)
            .collect();
        let excess = ordinary.len().saturating_sub(limit);
        for snapshot in ordinary.into_iter().take(excess) {
            repository
                .delete_snapshot(entry_id, &snapshot.id)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn emit_auto_backup_result(app: &AppHandle, entry_id: String, result: Result<(), String>) {
    let event = if result.is_ok() {
        "auto-backup-created"
    } else {
        "auto-backup-failed"
    };
    let _ = app.emit(
        event,
        serde_json::json!({ "archiveId": entry_id, "error": result.err() }),
    );
}

impl AutoBackupManager {
    pub fn new(repository: Arc<Mutex<LocalRepository>>, app: AppHandle) -> Self {
        Self {
            repository,
            app,
            watchers: Mutex::new(Vec::new()),
            pending: Arc::new(Mutex::new(HashMap::new())),
            restore_suppressions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn begin_restore_suppression(&self, entry_id: &str) -> Result<(), String> {
        self.pending
            .lock()
            .map_err(|_| "自动备份监听器不可用".to_owned())?
            .remove(entry_id);
        let mut suppressions = self
            .restore_suppressions
            .lock()
            .map_err(|_| "自动备份监听器不可用".to_owned())?;
        begin_restore_suppression(&mut suppressions, entry_id);
        Ok(())
    }

    pub fn finish_restore_suppression(&self, entry_id: &str) -> Result<(), String> {
        let delay = {
            let repository = self
                .repository
                .lock()
                .map_err(|_| "Chronicle 本地仓库状态不可用".to_owned())?;
            let settings = repository
                .load_settings()
                .map_err(|error| error.to_string())?;
            settings
                .pointer("/app/autoBackupDelaySeconds")
                .and_then(Value::as_u64)
                .unwrap_or(5)
                .clamp(1, 300)
        };
        let mut suppressions = self
            .restore_suppressions
            .lock()
            .map_err(|_| "自动备份监听器不可用".to_owned())?;
        finish_restore_suppression(
            &mut suppressions,
            entry_id,
            Instant::now(),
            Duration::from_secs(delay),
        );
        Ok(())
    }

    pub fn cancel_restore_suppression(&self, entry_id: &str) -> Result<(), String> {
        let mut suppressions = self
            .restore_suppressions
            .lock()
            .map_err(|_| "自动备份监听器不可用".to_owned())?;
        cancel_restore_suppression(&mut suppressions, entry_id);
        Ok(())
    }

    pub fn refresh(&self) -> Result<(), String> {
        self.watchers
            .lock()
            .map_err(|_| "自动备份监听器不可用".to_owned())?
            .clear();
        self.pending
            .lock()
            .map_err(|_| "自动备份监听器不可用".to_owned())?
            .clear();
        let (delay, retention, entries) = {
            let repository = self
                .repository
                .lock()
                .map_err(|_| "Chronicle 本地仓库状态不可用".to_owned())?;
            let settings = repository
                .load_settings()
                .map_err(|error| error.to_string())?;
            let app = settings.get("app").cloned().unwrap_or(Value::Null);
            let delay = app
                .get("autoBackupDelaySeconds")
                .and_then(Value::as_u64)
                .unwrap_or(5)
                .clamp(1, 300);
            let retention = app
                .get("retentionCount")
                .and_then(Value::as_u64)
                .map(|value| value as usize);
            let entries = repository
                .list_entries()
                .map_err(|error| error.to_string())?;
            (delay, retention, entries)
        };
        let entries: Vec<Entry> = entries
            .into_iter()
            .filter(|entry| {
                entry.auto_backup_enabled
                    && entry.sources.iter().any(|source| !source.path.is_empty())
            })
            .collect();
        for entry in entries.clone() {
            for source in &entry.sources {
                let path = PathBuf::from(&source.path);
                if !path.exists() {
                    continue;
                }
                let roots = entries.clone();
                let repository = self.repository.clone();
                let pending = self.pending.clone();
                let restore_suppressions = self.restore_suppressions.clone();
                let app = self.app.clone();
                let mut watcher =
                    notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                        let Ok(event) = event else {
                            return;
                        };
                        for entry in roots.iter().filter(|entry| {
                            entry.sources.iter().any(|source| {
                                let source_path = PathBuf::from(&source.path);
                                event.paths.iter().any(|changed| {
                                    changed.starts_with(&source_path)
                                        || (source_path.is_file() && changed == &source_path)
                                })
                            })
                        }) {
                            let suppressed = restore_suppressions.lock().ok().is_some_and(
                                |mut suppressions| {
                                    is_restore_suppressed(
                                        &mut suppressions,
                                        &entry.id,
                                        Instant::now(),
                                    )
                                },
                            );
                            if suppressed {
                                continue;
                            }
                            let should_create_initial_snapshot = {
                                let mut pending = pending.lock().expect("auto backup pending lock");
                                record_auto_backup_change(&mut pending, &entry.id)
                            };
                            if !should_create_initial_snapshot {
                                continue;
                            }
                            let repository = repository.clone();
                            let pending = pending.clone();
                            let app = app.clone();
                            let entry_id = entry.id.clone();
                            thread::spawn(move || {
                                emit_auto_backup_result(
                                    &app,
                                    entry_id.clone(),
                                    create_auto_backup_snapshot(&repository, &entry_id, retention),
                                );

                                thread::sleep(Duration::from_secs(delay));
                                let should_create_latest_snapshot =
                                    pending.lock().ok().is_some_and(|mut windows| {
                                        take_trailing_backup(&mut windows, &entry_id)
                                    });
                                if should_create_latest_snapshot {
                                    emit_auto_backup_result(
                                        &app,
                                        entry_id.clone(),
                                        create_auto_backup_snapshot(
                                            &repository,
                                            &entry_id,
                                            retention,
                                        ),
                                    );
                                }
                            });
                        }
                    })
                    .map_err(|error| error.to_string())?;
                watcher
                    .watch(
                        &path,
                        if path.is_dir() {
                            RecursiveMode::Recursive
                        } else {
                            RecursiveMode::NonRecursive
                        },
                    )
                    .map_err(|error| error.to_string())?;
                self.watchers
                    .lock()
                    .map_err(|_| "自动备份监听器不可用".to_owned())?
                    .push(watcher);
            }
        }
        Ok(())
    }
}

#[tauri::command(async)]
pub fn refresh_auto_backup(state: State<'_, AppState>) -> Result<(), String> {
    state.auto_backup.refresh()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        time::{Duration, Instant},
    };

    use super::{
        begin_restore_suppression, finish_restore_suppression, is_restore_suppressed,
        record_auto_backup_change, take_trailing_backup,
    };

    #[test]
    fn first_change_creates_an_immediate_snapshot_without_a_trailing_duplicate() {
        let mut windows = HashMap::new();

        assert!(record_auto_backup_change(&mut windows, "entry"));
        assert!(!take_trailing_backup(&mut windows, "entry"));
    }

    #[test]
    fn later_changes_in_the_window_create_one_latest_trailing_snapshot() {
        let mut windows = HashMap::new();

        assert!(record_auto_backup_change(&mut windows, "entry"));
        assert!(!record_auto_backup_change(&mut windows, "entry"));
        assert!(!record_auto_backup_change(&mut windows, "entry"));
        assert!(take_trailing_backup(&mut windows, "entry"));
    }

    #[test]
    fn restore_events_are_ignored_until_the_merge_window_ends() {
        let mut suppressions = HashMap::new();
        let now = Instant::now();

        begin_restore_suppression(&mut suppressions, "entry");
        assert!(is_restore_suppressed(&mut suppressions, "entry", now));

        finish_restore_suppression(&mut suppressions, "entry", now, Duration::from_secs(5));
        assert!(is_restore_suppressed(&mut suppressions, "entry", now));
        assert!(!is_restore_suppressed(
            &mut suppressions,
            "entry",
            now + Duration::from_secs(5) + Duration::from_millis(1),
        ));
    }
}
