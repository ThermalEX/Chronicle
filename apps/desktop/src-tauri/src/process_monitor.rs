use crate::game_exit::Observation;
use serde::Serialize;
use std::path::Path;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time: u64,
    pub executable_path: Option<String>,
    pub name: String,
}
#[derive(Serialize)]
pub struct ProcessSample {
    pub processes: Vec<ProcessIdentity>,
    pub complete: bool,
    pub partial: bool,
}

pub fn normalized_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .to_lowercase()
}
pub fn sample() -> ProcessSample {
    // A fresh enumeration avoids carrying dead processes over after a failed refresh.
    let mut system = System::new();
    let count = system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .without_tasks()
            .with_exe(UpdateKind::Always),
    );
    let processes: Vec<_> = system
        .processes()
        .values()
        .map(|p| ProcessIdentity {
            pid: p.pid().as_u32(),
            start_time: p.start_time(),
            executable_path: p.exe().map(|p| p.display().to_string()),
            name: p.name().to_string_lossy().into_owned(),
        })
        .collect();
    let partial = processes.iter().any(|p| p.executable_path.is_none());
    ProcessSample {
        processes,
        complete: count > 0,
        partial,
    }
}
pub fn observe(
    sample: &ProcessSample,
    target: &str,
    previous: &mut Vec<(u32, u64)>,
) -> Observation {
    if !sample.complete {
        return Observation::Unknown;
    }
    let matches: Vec<_> = sample
        .processes
        .iter()
        .filter(|p| {
            p.executable_path
                .as_ref()
                .is_some_and(|p| normalized_path(Path::new(p)) == target)
        })
        .map(|p| (p.pid, p.start_time))
        .collect();
    if !matches.is_empty() {
        *previous = matches;
        return Observation::Running;
    }
    let name = target.rsplit('\\').next().unwrap_or(target);
    if sample.processes.iter().any(|p| {
        p.executable_path.is_none()
            && (p.name.eq_ignore_ascii_case(name) || previous.contains(&(p.pid, p.start_time)))
    }) {
        return Observation::Unknown;
    }
    previous.clear();
    Observation::Stopped
}
#[tauri::command(async)]
pub fn list_backup_processes() -> Result<ProcessSample, String> {
    if !cfg!(windows) {
        return Err("windows_only".into());
    }
    let mut sample = sample();
    sample.processes.retain(|p| p.executable_path.is_some());
    Ok(sample)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Only launched by the isolated process integration test"]
    fn fixture_game_process() {
        let delay = std::env::var("CHRONICLE_TEST_GAME_MS")
            .unwrap()
            .parse::<u64>()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(delay.min(10_000)));
    }

    #[test]
    #[cfg(windows)]
    fn native_process_exit_drives_one_snapshot_in_isolated_repository() {
        use chronicle_storage::{AutomaticSnapshotOutcome, LocalRepository};
        use std::{
            fs,
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        let temporary = tempfile::tempdir().unwrap();
        let exe = temporary.path().join("Chronicle-test-game.exe");
        fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let spawn = |delay: &str| {
            Command::new(&exe)
                .args([
                    "--exact",
                    "process_monitor::tests::fixture_game_process",
                    "--ignored",
                ])
                .env("CHRONICLE_TEST_GAME_MS", delay)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x08000000)
                .spawn()
                .unwrap()
        };
        let mut first = spawn("2000");
        let mut second = spawn("4000");
        let target = normalized_path(&exe);
        let mut instances = vec![];
        let mut machine = crate::game_exit::GameExitState::default();
        let observation = observe(&sample(), &target, &mut instances);
        assert!(observation == Observation::Running);
        assert!(!machine.step(0, observation, 5_000, false));
        first.wait().unwrap();
        assert!(observe(&sample(), &target, &mut instances) == Observation::Running);
        second.wait().unwrap();
        let observation = observe(&sample(), &target, &mut instances);
        assert!(observation == Observation::Stopped);
        assert!(!machine.step(4000, observation, 5_000, false));
        let source = temporary.path().join("存档.txt");
        fs::write(&source, "saved on exit").unwrap();
        machine.changed(4500);
        assert!(!machine.step(9000, observation, 5_000, false));
        assert!(machine.step(9500, observation, 5_000, false));
        let token = machine.token;
        let repo = LocalRepository::open(temporary.path().join("repo")).unwrap();
        let entry = repo.add_entry(&source, None, None).unwrap();
        assert!(matches!(
            repo.create_automatic_snapshot(&entry.id, "exit", "test", &|| machine
                .can_commit(token, 9500))
                .unwrap(),
            AutomaticSnapshotOutcome::Created(_)
        ));
        machine.finish(token, "created");
        assert!(!machine.step(15000, observation, 5000, false));
        assert_eq!(repo.list_snapshots(&entry.id).unwrap().len(), 1);
        assert!(matches!(
            repo.create_automatic_snapshot(&entry.id, "exit", "test", &|| true)
                .unwrap(),
            AutomaticSnapshotOutcome::Unchanged
        ));
    }
    #[test]
    fn full_paths_instances_and_unknown_samples_are_distinct() {
        let target = normalized_path(Path::new(r"C:\games\game.exe"));
        let mut previous = vec![];
        let process = |pid, path: &str| ProcessIdentity {
            pid,
            start_time: 10,
            executable_path: Some(path.into()),
            name: "game.exe".into(),
        };
        let mut sample = ProcessSample {
            processes: vec![
                process(1, r"C:\games\game.exe"),
                process(2, r"C:\games\game.exe"),
            ],
            complete: true,
            partial: false,
        };
        assert!(observe(&sample, &target, &mut previous) == Observation::Running);
        sample.processes.remove(0);
        assert!(observe(&sample, &target, &mut previous) == Observation::Running);
        sample.processes[0].executable_path = None;
        assert!(observe(&sample, &target, &mut previous) == Observation::Unknown);
        sample.processes = vec![process(3, r"C:\other\game.exe")];
        assert!(observe(&sample, &target, &mut previous) == Observation::Stopped);
        sample.complete = false;
        assert!(observe(&sample, &target, &mut previous) == Observation::Unknown);
    }
}
