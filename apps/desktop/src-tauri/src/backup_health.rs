use chronicle_storage::health::{
    ContentComparison, HealthCode, HealthEntryResult, PendingObservation, inspect_entry,
    update_observation,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager, State};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    format_version: u32,
    started_at: u64,
    completed_at: u64,
    entries: Vec<HealthEntryResult>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthTaskState {
    task_id: String,
    revision: u64,
    status: String,
    checked: usize,
    total: usize,
    current_entry_id: Option<String>,
    entries: Vec<HealthEntryResult>,
    error: Option<String>,
}
impl Default for HealthTaskState {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            revision: 0,
            status: "idle".into(),
            checked: 0,
            total: 0,
            current_entry_id: None,
            entries: vec![],
            error: None,
        }
    }
}
#[derive(Default)]
struct Task {
    state: HealthTaskState,
    cancelled: Arc<AtomicBool>,
}
#[derive(Default)]
pub struct HealthTasks(Mutex<Task>);
impl HealthTasks {
    fn begin(&self) -> Result<(String, Option<Arc<AtomicBool>>), String> {
        let mut task = self.0.lock().map_err(|e| e.to_string())?;
        if matches!(task.state.status.as_str(), "running" | "cancelling") {
            return Ok((task.state.task_id.clone(), None));
        }
        let revision = task.state.revision + 1;
        task.state = HealthTaskState {
            task_id: uuid::Uuid::new_v4().to_string(),
            status: "running".into(),
            revision,
            ..Default::default()
        };
        task.cancelled = Arc::new(AtomicBool::new(false));
        Ok((task.state.task_id.clone(), Some(task.cancelled.clone())))
    }
    fn state(&self) -> Result<HealthTaskState, String> {
        self.0
            .lock()
            .map(|t| t.state.clone())
            .map_err(|e| e.to_string())
    }
    fn cancel(&self, id: &str) -> Result<(), String> {
        let mut task = self.0.lock().map_err(|e| e.to_string())?;
        if task.state.task_id == id && task.state.status == "running" {
            task.cancelled.store(true, Ordering::Relaxed);
            task.state.status = "cancelling".into();
            task.state.revision += 1;
        }
        Ok(())
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path.parent().ok_or("invalid_cache_path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let result = fs::rename(&tmp, path).map_err(|e| e.to_string());
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Observations {
    format_version: u32,
    entries: BTreeMap<String, PendingObservation>,
}
fn publish(app: &tauri::AppHandle, id: &str, change: impl FnOnce(&mut HealthTaskState)) {
    let tasks = app.state::<HealthTasks>();
    if let Ok(mut task) = tasks.0.lock() {
        if task.state.task_id != id {
            return;
        }
        change(&mut task.state);
        task.state.revision += 1;
        let _ = app.emit("backup-health-progress", task.state.clone());
    }
}
#[tauri::command]
pub fn get_backup_health_state(tasks: State<'_, HealthTasks>) -> Result<HealthTaskState, String> {
    tasks.state()
}
#[tauri::command]
pub fn cancel_backup_health_check(
    app: tauri::AppHandle,
    tasks: State<'_, HealthTasks>,
    task_id: String,
) -> Result<(), String> {
    tasks.cancel(&task_id)?;
    let _ = app.emit("backup-health-progress", tasks.state()?);
    Ok(())
}
#[tauri::command]
pub async fn load_backup_health_report(
    state: State<'_, crate::AppState>,
) -> Result<Option<HealthReport>, String> {
    load_report_from_repository(state.repository.clone()).await
}

async fn load_report_from_repository(
    repository: Arc<Mutex<chronicle_storage::LocalRepository>>,
) -> Result<Option<HealthReport>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = repository
            .lock()
            .map_err(|e| e.to_string())?
            .root()
            .to_path_buf();
        match fs::read(root.join("cache/backup-health-report.json")) {
            Ok(bytes) => {
                let report: HealthReport =
                    serde_json::from_slice(&bytes).map_err(|_| "health_cache_corrupt")?;
                if report.format_version != 1 {
                    return Err("health_cache_corrupt".into());
                }
                Ok(Some(report))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    })
    .await
    .map_err(|error| error.to_string())?
}
#[tauri::command]
pub fn start_backup_health_check(
    app: tauri::AppHandle,
    state: State<'_, crate::AppState>,
    tasks: State<'_, HealthTasks>,
) -> Result<String, String> {
    if !cfg!(windows) {
        return Err("windows_only".into());
    }
    let (id, flag) = tasks.begin()?;
    let Some(flag) = flag else {
        return Ok(id);
    };
    let repository = state.repository.clone();
    let task_id = id.clone();
    std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let started = now();
            let (repo, inputs, days) = {
                let repo = repository.lock().map_err(|e| e.to_string())?;
                let days = repo
                    .load_settings()
                    .ok()
                    .and_then(|s| {
                        s.pointer("/app/backupHealthStaleDays")
                            .and_then(serde_json::Value::as_u64)
                    })
                    .unwrap_or(7)
                    .clamp(1, 365) as u16;
                (
                    repo.clone(),
                    repo.health_inputs().map_err(|e| e.to_string())?,
                    days,
                )
            };
            publish(&app, &task_id, |s| s.total = inputs.len());
            let observations_path = repo.root().join("config/backup-health-observations.json");
            let mut observations: Observations = match fs::read(&observations_path) {
                Ok(bytes) => match serde_json::from_slice::<Observations>(&bytes) {
                    Ok(o) if o.format_version == 1 => o,
                    _ => {
                        publish(&app, &task_id, |s| {
                            s.error = Some("health_cache_corrupt".into())
                        });
                        Observations::default()
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Observations::default(),
                Err(e) => return Err(e.to_string()),
            };
            observations.format_version = 1;
            let mut results = vec![];
            for input in &inputs {
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                publish(&app, &task_id, |s| {
                    s.current_entry_id = Some(input.entry.id.clone())
                });
                let mut result = inspect_entry(input, &flag);
                let current = repository
                    .lock()
                    .map_err(|e| e.to_string())?
                    .health_input(&input.entry.id)
                    .map_err(|e| e.to_string())?;
                if !current.is_some_and(|item| item.fingerprint == input.fingerprint) {
                    result.code = HealthCode::ChangedDuringCheck;
                    result.comparison = ContentComparison::Unknown;
                }
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                let update = update_observation(
                    observations.entries.remove(&input.entry.id),
                    &result.comparison,
                    &input.fingerprint,
                    now(),
                    days,
                );
                if let Some(observation) = update.observation {
                    observations
                        .entries
                        .insert(input.entry.id.clone(), observation);
                }
                if update.stale && result.code == HealthCode::PendingChanges {
                    result.code = HealthCode::StaleChanges;
                }
                results.push(result);
                publish(&app, &task_id, |s| {
                    s.entries = results.clone();
                    s.checked = results.len();
                });
            }
            // Revalidate earlier entries too: a restore or backup may have happened
            // while later archives were being checked. Do not publish a stale success.
            let guard = repository.lock().map_err(|e| e.to_string())?;
            let current = guard.health_inputs().map_err(|e| e.to_string())?;
            let fingerprints: BTreeMap<_, _> = current
                .iter()
                .map(|item| (&item.entry.id, &item.fingerprint))
                .collect();
            for (input, result) in inputs.iter().zip(results.iter_mut()) {
                if fingerprints.get(&input.entry.id).copied() != Some(&input.fingerprint) {
                    result.code = HealthCode::ChangedDuringCheck;
                    result.comparison = ContentComparison::Unknown;
                    observations.entries.remove(&input.entry.id);
                }
            }
            // Cancellation and committing the complete report share a lock.
            let tasks = app.state::<HealthTasks>();
            let mut task = tasks.0.lock().map_err(|e| e.to_string())?;
            if flag.load(Ordering::Relaxed) {
                task.state.status = "cancelled".into();
            } else {
                task.state.entries = results.clone();
                atomic_json(&observations_path, &observations)?;
                atomic_json(
                    &repo.root().join("cache/backup-health-report.json"),
                    &HealthReport {
                        format_version: 1,
                        started_at: started,
                        completed_at: now(),
                        entries: results,
                    },
                )?;
                task.state.status = "completed".into();
            }
            task.state.current_entry_id = None;
            task.state.revision += 1;
            let _ = app.emit("backup-health-progress", task.state.clone());
            Ok(())
        };
        if let Err(error) = run() {
            publish(&app, &task_id, |s| {
                s.status = "failed".into();
                s.error = Some(error);
            });
        }
    });
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_read_yields_instead_of_waiting_on_repository_in_async_executor() {
        use std::{
            future::Future,
            task::{Context, Poll, Waker},
        };
        let temp = tempfile::tempdir().unwrap();
        let repository = Arc::new(Mutex::new(
            chronicle_storage::LocalRepository::open(temp.path()).unwrap(),
        ));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let busy_repository = repository.clone();
        let worker = std::thread::spawn(move || {
            let _guard = busy_repository.lock().unwrap();
            ready_tx.send(()).unwrap();
            // Bounded fallback makes the old blocking implementation fail rather than hang.
            let _ = release_rx.recv_timeout(std::time::Duration::from_secs(2));
        });
        ready_rx.recv().unwrap();
        let mut future = std::pin::pin!(load_report_from_repository(repository));
        let initial = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        let _ = release_tx.send(());
        worker.join().unwrap();
        assert!(
            matches!(initial, Poll::Pending),
            "report IO must yield while repository is busy"
        );
        assert!(tauri::async_runtime::block_on(future).unwrap().is_none());
    }

    #[test]
    fn duplicate_start_and_wrong_cancel_do_not_replace_current_task() {
        let tasks = HealthTasks::default();
        let (id, _) = tasks.begin().unwrap();
        assert_eq!(tasks.begin().unwrap().0, id);
        tasks.cancel("wrong").unwrap();
        assert_eq!(tasks.state().unwrap().status, "running");
        tasks.cancel(&id).unwrap();
        assert_eq!(tasks.state().unwrap().status, "cancelling");
    }
}
