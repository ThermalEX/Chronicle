use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TutorialProgress {
    status: TutorialStatus,
    #[serde(default)]
    seen_tips: Vec<TutorialTip>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum TutorialStatus {
    Pending,
    Completed,
    Skipped,
    Legacy,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum TutorialTip {
    Categories,
    Exclusions,
    Registry,
}

pub fn initialize(root: &Path) -> Result<(), String> {
    if root.join("config/onboarding-local.json").exists() {
        return Ok(());
    }
    let status = if root.join("library.json").exists() || root.join("catalog.json").exists() {
        TutorialStatus::Legacy
    } else {
        TutorialStatus::Pending
    };
    write(
        root,
        &TutorialProgress {
            status,
            seen_tips: Vec::new(),
        },
    )
}
fn read(root: &Path) -> Result<TutorialProgress, String> {
    let bytes =
        fs::read(root.join("config/onboarding-local.json")).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}
fn write(root: &Path, progress: &TutorialProgress) -> Result<(), String> {
    use std::io::Write;
    let directory = root.join("config");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let temporary = directory.join(format!(".onboarding-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(progress).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&temporary, directory.join("onboarding-local.json"))
            .map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[tauri::command]
pub fn load_onboarding(
    state: tauri::State<'_, crate::AppState>,
) -> Result<TutorialProgress, String> {
    let repository = state.repository.lock().map_err(|error| error.to_string())?;
    read(repository.root())
}
#[tauri::command]
pub fn save_onboarding(
    state: tauri::State<'_, crate::AppState>,
    progress: TutorialProgress,
) -> Result<(), String> {
    let repository = state.repository.lock().map_err(|error| error.to_string())?;
    write(repository.root(), &progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Scope(std::path::PathBuf);
    impl Scope {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("chronicle-tutorial-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scope {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn fresh_repository_remains_pending_after_initialization_and_reopen() {
        let scope = Scope::new();
        initialize(&scope.0).unwrap();
        fs::write(scope.0.join("library.json"), b"{}").unwrap();
        initialize(&scope.0).unwrap();
        assert_eq!(read(&scope.0).unwrap().status, TutorialStatus::Pending);
    }
    #[test]
    fn existing_empty_repository_is_not_a_new_user() {
        let scope = Scope::new();
        fs::write(scope.0.join("library.json"), b"{}").unwrap();
        initialize(&scope.0).unwrap();
        assert_eq!(read(&scope.0).unwrap().status, TutorialStatus::Legacy);
    }
    #[test]
    fn completion_and_tips_survive_reopen_without_entering_settings() {
        let scope = Scope::new();
        initialize(&scope.0).unwrap();
        let progress = TutorialProgress {
            status: TutorialStatus::Completed,
            seen_tips: vec![TutorialTip::Registry],
        };
        write(&scope.0, &progress).unwrap();
        initialize(&scope.0).unwrap();
        assert_eq!(read(&scope.0).unwrap(), progress);
        assert!(!scope.0.join("config/app-settings.json").exists());
    }
    #[test]
    fn malformed_progress_is_not_silently_reset_to_pending() {
        let scope = Scope::new();
        fs::create_dir_all(scope.0.join("config")).unwrap();
        fs::write(scope.0.join("config/onboarding-local.json"), b"bad").unwrap();
        initialize(&scope.0).unwrap();
        assert!(read(&scope.0).is_err());
    }
}
