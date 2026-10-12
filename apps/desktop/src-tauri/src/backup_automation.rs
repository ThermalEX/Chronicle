use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};
use tauri::State;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerMode {
    #[default]
    FileChange,
    GameExit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupTriggerConfig {
    pub mode: TriggerMode,
    pub executable_path: Option<String>,
    pub quiet_seconds: u16,
}
impl Default for BackupTriggerConfig {
    fn default() -> Self {
        Self {
            mode: TriggerMode::FileChange,
            executable_path: None,
            quiet_seconds: 5,
        }
    }
}
impl BackupTriggerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=300).contains(&self.quiet_seconds) {
            return Err("backup_invalid_quiet_seconds".into());
        }
        if self.mode == TriggerMode::GameExit {
            let path = Path::new(
                self.executable_path
                    .as_deref()
                    .ok_or("backup_executable_required")?,
            );
            if !path.is_absolute()
                || !path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            {
                return Err("backup_invalid_executable".into());
            }
            if !chronicle_storage::health::safe_metadata(path).is_ok_and(|m| m.is_file()) {
                return Err("backup_executable_unavailable".into());
            }
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerDocument {
    format_version: u32,
    pub entries: BTreeMap<String, BackupTriggerConfig>,
}
pub fn load_triggers(root: &Path) -> Result<TriggerDocument, String> {
    match fs::read(root.join("config/backup-automation.json")) {
        Ok(bytes) => {
            let document: TriggerDocument =
                serde_json::from_slice(&bytes).map_err(|_| "backup_config_corrupt")?;
            if document.format_version != 1
                || document.entries.values().any(|c| {
                    !(1..=300).contains(&c.quiet_seconds)
                        || (c.mode == TriggerMode::GameExit
                            && c.executable_path.as_deref().is_none_or(str::is_empty))
                })
            {
                return Err("backup_config_corrupt".into());
            }
            Ok(document)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(TriggerDocument {
            format_version: 1,
            entries: BTreeMap::new(),
        }),
        Err(e) => Err(e.to_string()),
    }
}
fn save_triggers(root: &Path, document: &TriggerDocument) -> Result<(), String> {
    crate::atomic_file::write_json_pretty(&root.join("config/backup-automation.json"), document)
}
#[tauri::command]
pub fn get_backup_trigger(
    state: State<'_, crate::AppState>,
    entry_id: String,
) -> Result<BackupTriggerConfig, String> {
    let repo = state.repository.lock().map_err(|e| e.to_string())?;
    repo.get_entry(&entry_id).map_err(|e| e.to_string())?;
    Ok(load_triggers(repo.root())?
        .entries
        .remove(&entry_id)
        .unwrap_or_default())
}
#[tauri::command]
pub fn set_backup_trigger(
    state: State<'_, crate::AppState>,
    entry_id: String,
    config: BackupTriggerConfig,
) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("windows_only".into());
    }
    config.validate()?;
    {
        let repo = state.repository.lock().map_err(|e| e.to_string())?;
        repo.get_entry(&entry_id).map_err(|e| e.to_string())?;
        let mut document = load_triggers(repo.root())?;
        document.entries.insert(entry_id, config);
        save_triggers(repo.root(), &document)?;
    }
    state.auto_backup.refresh()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_config_is_legacy_but_corruption_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_triggers(dir.path()).unwrap().entries.len(), 0);
        std::fs::create_dir(dir.path().join("config")).unwrap();
        std::fs::write(dir.path().join("config/backup-automation.json"), "broken").unwrap();
        assert!(load_triggers(dir.path()).is_err());
        assert_eq!(BackupTriggerConfig::default().mode, TriggerMode::FileChange);
    }
    #[test]
    fn exit_trigger_requires_real_absolute_executable_and_valid_delay() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("game.exe");
        let mut config = BackupTriggerConfig {
            mode: TriggerMode::GameExit,
            executable_path: Some(exe.display().to_string()),
            quiet_seconds: 5,
        };
        assert!(config.validate().is_err());
        std::fs::write(&exe, "test fixture").unwrap();
        assert!(config.validate().is_ok());
        config.quiet_seconds = 0;
        assert!(config.validate().is_err());
        config.quiet_seconds = 301;
        assert!(config.validate().is_err());
        config.quiet_seconds = 5;
        config.executable_path = Some("game.exe".into());
        assert!(config.validate().is_err());
    }
}
