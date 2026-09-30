use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use chronicle_core::{Entry, EntryKind, Snapshot};
use chronicle_storage::{AutomaticSnapshotOutcome, LocalRepository, exclusions::ExclusionRules};
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
    restore_revisions: Arc<Mutex<HashMap<String, u64>>>,
    generation: Arc<AtomicU64>,
    games: Arc<Mutex<HashMap<String, GameRuntime>>>,
    epoch: Instant,
    refresh_lock: Mutex<()>,
}

struct GameRuntime {
    state: crate::game_exit::GameExitState,
    target: String,
    quiet: u64,
    instances: Vec<(u32, u64)>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRuntimeStatus {
    entry_id: String,
    generation: u64,
    status: String,
    reason_code: Option<String>,
}

fn runtime_status(id: &str, generation: u64, game: &GameRuntime) -> BackupRuntimeStatus {
    BackupRuntimeStatus {
        entry_id: id.into(),
        generation,
        status: game.state.status.into(),
        reason_code: game.state.reason.map(str::to_owned),
    }
}

/// Returns whether this change starts an archive's coalescing window.
/// The window creates exactly one snapshot after its delay, using the latest file state.
fn record_auto_backup_change(windows: &mut HashMap<String, bool>, entry_id: &str) -> bool {
    windows.insert(entry_id.to_owned(), true).is_none()
}

fn take_trailing_backup(windows: &mut HashMap<String, bool>, entry_id: &str) -> bool {
    windows.remove(entry_id).is_some()
}

/// Blocks watcher events while a restore is writing, then for one merge window afterwards.
/// `None` represents an in-progress restore whose completion time is not known yet.
fn begin_restore_suppression(suppressions: &mut HashMap<String, Option<Instant>>, entry_id: &str) {
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

fn cancel_restore_suppression(suppressions: &mut HashMap<String, Option<Instant>>, entry_id: &str) {
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

fn retention_candidates(snapshots: &[Snapshot], limit: usize) -> Vec<String> {
    let mut ordinary: Vec<_> = snapshots
        .iter()
        .filter(|snapshot| !snapshot.safety && !snapshot.locked)
        .collect();
    ordinary.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.created_at_ms));
    ordinary
        .into_iter()
        .skip(limit)
        .map(|snapshot| snapshot.id.clone())
        .collect()
}

fn entry_accepts_event(entry: &Entry, event: &notify::Event) -> bool {
    if !matches!(
        event.kind,
        notify::EventKind::Create(_) | notify::EventKind::Modify(_) | notify::EventKind::Remove(_)
    ) {
        return false;
    }
    let Ok(rules) = ExclusionRules::new(&entry.exclude_patterns) else {
        return false;
    };
    entry
        .sources
        .iter()
        .filter(|source| source.kind != EntryKind::Registry)
        .any(|source| {
            let root = PathBuf::from(&source.path);
            event.paths.iter().any(|changed| {
                if source.kind == EntryKind::File {
                    changed == &root
                        && !rules.is_excluded(std::path::Path::new(&source.name), false)
                } else {
                    let is_dir = changed.is_dir()
                        || matches!(
                            event.kind,
                            notify::EventKind::Remove(notify::event::RemoveKind::Folder)
                        );
                    changed
                        .strip_prefix(&root)
                        .is_ok_and(|relative| !rules.is_excluded(relative, is_dir))
                }
            })
        })
}

fn create_auto_backup_snapshot(
    repository: &Arc<Mutex<LocalRepository>>,
    entry_id: &str,
    retention: Option<usize>,
    can_commit: &dyn Fn() -> bool,
) -> Result<AutomaticSnapshotOutcome, String> {
    let repository = repository
        .lock()
        .map_err(|_| "Chronicle 本地仓库状态不可用".to_owned())?;
    let (device, _) = repository
        .device_identity()
        .map_err(|error| error.to_string())?;
    let outcome = repository
        .create_automatic_snapshot(entry_id, "自动备份", &device, can_commit)
        .map_err(|error| error.to_string())?;
    if !matches!(outcome, AutomaticSnapshotOutcome::Created(_)) {
        return Ok(outcome);
    }
    if let Some(limit) = retention {
        let snapshots = repository
            .list_snapshots(entry_id)
            .map_err(|error| error.to_string())?;
        for snapshot_id in retention_candidates(&snapshots, limit) {
            repository
                .delete_snapshot_with_reason(entry_id, &snapshot_id, "retention")
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(outcome)
}

fn emit_auto_backup_result(
    app: &AppHandle,
    entry_id: String,
    result: Result<AutomaticSnapshotOutcome, String>,
) {
    let event = match &result {
        Ok(AutomaticSnapshotOutcome::Created(_)) => "auto-backup-created",
        Ok(AutomaticSnapshotOutcome::Unchanged) => "auto-backup-unchanged",
        Ok(AutomaticSnapshotOutcome::Superseded) => return,
        Err(_) => "auto-backup-failed",
    };
    let _ = app.emit(
        event,
        serde_json::json!({ "archiveId": entry_id, "error": result.err() }),
    );
}

impl AutoBackupManager {
    fn start_game_monitor(&self, generation: u64, retention: Option<usize>) {
        let games = self.games.clone();
        let live_generation = self.generation.clone();
        let repository = self.repository.clone();
        let app = self.app.clone();
        let suppressions = self.restore_suppressions.clone();
        let epoch = self.epoch;
        thread::spawn(move || {
            while live_generation.load(Ordering::SeqCst) == generation {
                let sample = crate::process_monitor::sample();
                let now = epoch.elapsed().as_millis() as u64;
                let mut requests = vec![];
                if let Ok(mut games) = games.lock() {
                    if live_generation.load(Ordering::SeqCst) != generation {
                        break;
                    }
                    for (id, game) in games.iter_mut() {
                        if game.state.reason == Some("backup_executable_unavailable") {
                            let _ = app.emit(
                                "backup-automation-state",
                                runtime_status(id, generation, game),
                            );
                            continue;
                        }
                        let suppressed = suppressions
                            .lock()
                            .ok()
                            .is_none_or(|mut s| is_restore_suppressed(&mut s, id, Instant::now()));
                        let observation = if std::path::Path::new(&game.target).is_file() {
                            crate::process_monitor::observe(
                                &sample,
                                &game.target,
                                &mut game.instances,
                            )
                        } else {
                            crate::game_exit::Observation::Unknown
                        };
                        let before = (game.state.status, game.state.reason);
                        if game.state.step(now, observation, game.quiet, suppressed) {
                            requests.push((id.clone(), game.state.token));
                        }
                        if before != (game.state.status, game.state.reason) {
                            let _ = app.emit(
                                "backup-automation-state",
                                runtime_status(id, generation, game),
                            );
                        }
                    }
                }
                for (entry_id, token) in requests {
                    let games = games.clone();
                    let live_generation = live_generation.clone();
                    let repository = repository.clone();
                    let app = app.clone();
                    let suppressions = suppressions.clone();
                    thread::spawn(move || {
                        let result =
                            create_auto_backup_snapshot(&repository, &entry_id, retention, &|| {
                                if live_generation.load(Ordering::SeqCst) != generation {
                                    return false;
                                }
                                let allowed = games.lock().ok().is_some_and(|games| {
                                    games.get(&entry_id).is_some_and(|game| {
                                        game.state
                                            .can_commit(token, epoch.elapsed().as_millis() as u64)
                                    })
                                });
                                allowed
                                    && suppressions.lock().ok().is_some_and(|mut s| {
                                        !is_restore_suppressed(&mut s, &entry_id, Instant::now())
                                    })
                            });
                        if live_generation.load(Ordering::SeqCst) == generation {
                            if let Ok(mut games) = games.lock() {
                                if let Some(game) = games.get_mut(&entry_id) {
                                    let outcome = match &result {
                                        Ok(AutomaticSnapshotOutcome::Created(_)) => "created",
                                        Ok(AutomaticSnapshotOutcome::Unchanged) => "unchanged",
                                        Ok(AutomaticSnapshotOutcome::Superseded) => "superseded",
                                        Err(_) => "failed",
                                    };
                                    game.state.finish(token, outcome);
                                    let _ = app.emit(
                                        "backup-automation-state",
                                        runtime_status(&entry_id, generation, game),
                                    );
                                }
                            }
                        }
                        emit_auto_backup_result(&app, entry_id, result);
                    });
                }
                thread::sleep(Duration::from_secs(2));
            }
        });
    }

    pub fn runtime_states(&self) -> Result<Vec<BackupRuntimeStatus>, String> {
        let generation = self.generation.load(Ordering::SeqCst);
        Ok(self
            .games
            .lock()
            .map_err(|e| e.to_string())?
            .iter()
            .map(|(id, game)| runtime_status(id, generation, game))
            .collect())
    }

    pub fn new(repository: Arc<Mutex<LocalRepository>>, app: AppHandle) -> Self {
        Self {
            repository,
            app,
            watchers: Mutex::new(Vec::new()),
            pending: Arc::new(Mutex::new(HashMap::new())),
            restore_suppressions: Arc::new(Mutex::new(HashMap::new())),
            restore_revisions: Arc::new(Mutex::new(HashMap::new())),
            generation: Arc::new(AtomicU64::new(0)),
            games: Arc::new(Mutex::new(HashMap::new())),
            epoch: Instant::now(),
            refresh_lock: Mutex::new(()),
        }
    }

    pub fn begin_restore_suppression(&self, entry_id: &str) -> Result<(), String> {
        *self
            .restore_revisions
            .lock()
            .map_err(|e| e.to_string())?
            .entry(entry_id.into())
            .or_default() += 1;
        if let Some(game) = self
            .games
            .lock()
            .map_err(|e| e.to_string())?
            .get_mut(entry_id)
        {
            game.state.suppress();
        }
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
        let _refresh = self.refresh_lock.lock().map_err(|e| e.to_string())?;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mut previous_games =
            std::mem::take(&mut *self.games.lock().map_err(|e| e.to_string())?);
        let _ = self.app.emit(
            "backup-automation-reset",
            serde_json::json!({"generation":generation}),
        );
        self.watchers
            .lock()
            .map_err(|_| "自动备份监听器不可用".to_owned())?
            .clear();
        self.pending
            .lock()
            .map_err(|_| "自动备份监听器不可用".to_owned())?
            .clear();
        let (delay, retention, entries, triggers) = {
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
            let triggers = crate::backup_automation::load_triggers(repository.root())?;
            (delay, retention, entries, triggers)
        };
        let entries: Vec<Entry> = entries
            .into_iter()
            .filter(|entry| entry.auto_backup_enabled)
            .collect();
        for entry in &entries {
            let config = triggers.entries.get(&entry.id).cloned().unwrap_or_default();
            if config.mode == crate::backup_automation::TriggerMode::GameExit {
                let mut state = crate::game_exit::GameExitState::default();
                if config.validate().is_err() {
                    state.status = "needs_attention";
                    state.reason = Some("backup_executable_unavailable");
                }
                let target = crate::process_monitor::normalized_path(std::path::Path::new(
                    config.executable_path.as_deref().unwrap_or_default(),
                ));
                let quiet = u64::from(config.quiet_seconds) * 1000;
                let mut game = previous_games
                    .remove(&entry.id)
                    .filter(|old| {
                        old.target == target && old.quiet == quiet && state.reason.is_none()
                    })
                    .unwrap_or(GameRuntime {
                        state,
                        target,
                        quiet,
                        instances: vec![],
                    });
                // Old workers are invalidated, but unrelated settings refreshes must
                // not discard a game session whose binding is unchanged.
                if game.state.status == "backing_up" {
                    game.state.changed(self.epoch.elapsed().as_millis() as u64);
                }
                self.games
                    .lock()
                    .map_err(|e| e.to_string())?
                    .insert(entry.id.clone(), game);
            }
        }
        for status in self.runtime_states()? {
            let _ = self.app.emit("backup-automation-state", status);
        }
        if !self.games.lock().map_err(|e| e.to_string())?.is_empty() {
            self.start_game_monitor(generation, retention);
        }
        for entry in entries.clone() {
            for source in &entry.sources {
                if source.kind == EntryKind::Registry {
                    continue;
                }
                let path = PathBuf::from(&source.path);
                if !path.exists() {
                    continue;
                }
                let roots = entries.clone();
                let repository = self.repository.clone();
                let pending = self.pending.clone();
                let restore_suppressions = self.restore_suppressions.clone();
                let restore_revisions = self.restore_revisions.clone();
                let app = self.app.clone();
                let live_generation = self.generation.clone();
                let games = self.games.clone();
                let epoch = self.epoch;
                let mut watcher =
                    notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                        if live_generation.load(Ordering::SeqCst) != generation {
                            return;
                        }
                        let Ok(event) = event else {
                            return;
                        };
                        for entry in roots
                            .iter()
                            .filter(|entry| entry_accepts_event(entry, &event))
                        {
                            let suppressed =
                                restore_suppressions
                                    .lock()
                                    .ok()
                                    .is_some_and(|mut suppressions| {
                                        is_restore_suppressed(
                                            &mut suppressions,
                                            &entry.id,
                                            Instant::now(),
                                        )
                                    });
                            if suppressed {
                                continue;
                            }
                            if let Ok(mut games) = games.lock() {
                                if let Some(game) = games.get_mut(&entry.id) {
                                    game.state.changed(epoch.elapsed().as_millis() as u64);
                                    continue;
                                }
                            }
                            let should_start_merge_window = {
                                let mut pending = pending.lock().expect("auto backup pending lock");
                                record_auto_backup_change(&mut pending, &entry.id)
                            };
                            if !should_start_merge_window {
                                continue;
                            }
                            let repository = repository.clone();
                            let pending = pending.clone();
                            let app = app.clone();
                            let entry_id = entry.id.clone();
                            let live_generation = live_generation.clone();
                            let suppressions = restore_suppressions.clone();
                            let restore_revisions = restore_revisions.clone();
                            let restore_revision = restore_revisions
                                .lock()
                                .ok()
                                .and_then(|r| r.get(&entry_id).copied())
                                .unwrap_or(0);
                            thread::spawn(move || {
                                thread::sleep(Duration::from_secs(delay));
                                if live_generation.load(Ordering::SeqCst) != generation {
                                    return;
                                }
                                if !restore_revisions.lock().ok().is_some_and(|r| {
                                    r.get(&entry_id).copied().unwrap_or(0) == restore_revision
                                }) {
                                    return;
                                }
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
                                            &|| {
                                                live_generation.load(Ordering::SeqCst) == generation
                                                    && restore_revisions.lock().ok().is_some_and(
                                                        |r| {
                                                            r.get(&entry_id).copied().unwrap_or(0)
                                                                == restore_revision
                                                        },
                                                    )
                                                    && suppressions.lock().ok().is_some_and(
                                                        |mut s| {
                                                            !is_restore_suppressed(
                                                                &mut s,
                                                                &entry_id,
                                                                Instant::now(),
                                                            )
                                                        },
                                                    )
                                            },
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

#[tauri::command]
pub fn get_backup_runtime_states(
    state: State<'_, AppState>,
) -> Result<Vec<BackupRuntimeStatus>, String> {
    state.auto_backup.runtime_states()
}

#[cfg(test)]
mod tests {
    #[test]
    fn exclusions_filter_watcher_events_but_registry_paths_never_trigger() {
        let entry: chronicle_core::Entry = serde_json::from_value(serde_json::json!({
            "id":"entry","name":"Save","category_id":null,"storage_policy":"local","created_at_ms":0,
            "exclude_patterns":["*.tmp","cache/"],
            "sources":[{"id":"folder","name":"save","kind":"directory","path":"C:\\save"},
                       {"id":"reg","name":"Registry","kind":"registry","path":"HKEY_CURRENT_USER\\Software\\Test"}]
        })).unwrap();
        let change = |path: &str| {
            notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
                .add_path(path.into())
        };
        assert!(!super::entry_accepts_event(
            &entry,
            &change(r"C:\save\cache\gone.dat")
        ));
        assert!(!super::entry_accepts_event(
            &entry,
            &change(r"C:\save\log.tmp")
        ));
        assert!(super::entry_accepts_event(
            &entry,
            &change(r"C:\save\progress.dat")
        ));
        assert!(!super::entry_accepts_event(
            &entry,
            &change(r"HKEY_CURRENT_USER\Software\Test")
        ));
        assert!(!super::entry_accepts_event(
            &entry,
            &notify::Event::new(notify::EventKind::Access(notify::event::AccessKind::Any))
                .add_path(r"C:\save\progress.dat".into())
        ));
    }

    #[test]
    fn retention_keeps_locked_and_safety_snapshots_outside_the_normal_limit() {
        let items = vec![
            serde_json::json!({"id":"old","safety":false,"locked":false}),
            serde_json::json!({"id":"locked","safety":false,"locked":true}),
            serde_json::json!({"id":"new","safety":false,"locked":false}),
            serde_json::json!({"id":"safety","safety":true,"locked":false}),
        ];
        let snapshots: Vec<chronicle_core::Snapshot> = items.into_iter().enumerate().map(|(time,mut value)| {
            let object = value.as_object_mut().unwrap();
            object.extend(serde_json::json!({"entry_id":"e","parent_id":null,"device_id":"test","title":"test","note":"", "created_at_ms":time,"archive_name":"a.7z","object_hash":"hash","size_bytes":1,"files":[],"changes":{"added":0,"modified":0,"deleted":0}}).as_object().unwrap().clone());
            serde_json::from_value(value).unwrap()
        }).collect();
        assert_eq!(super::retention_candidates(&snapshots, 1), vec!["old"]);
    }
    use std::{
        collections::HashMap,
        time::{Duration, Instant},
    };

    use super::{
        begin_restore_suppression, finish_restore_suppression, is_restore_suppressed,
        record_auto_backup_change, take_trailing_backup,
    };

    #[test]
    fn first_change_leaves_one_latest_snapshot_pending_for_the_merge_window() {
        let mut windows = HashMap::new();

        assert!(record_auto_backup_change(&mut windows, "entry"));
        assert!(take_trailing_backup(&mut windows, "entry"));
    }

    #[test]
    fn later_changes_in_the_window_still_leave_only_one_latest_snapshot_pending() {
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
