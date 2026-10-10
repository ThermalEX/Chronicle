use std::{
    io,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};

pub(crate) fn isolate_portable_webviews(
    executable: &Path,
    config: &mut tauri::Config,
) -> io::Result<Vec<tauri::utils::config::WindowConfig>> {
    let directory = executable
        .parent()
        .ok_or_else(|| io::Error::other("executable has no parent directory"))?;
    if !directory.join("portable.marker").is_file() {
        return Ok(Vec::new());
    }
    let profile = directory.join("Chronicle-data/webview");
    std::fs::create_dir_all(&profile)?;
    let mut windows = Vec::new();
    for window in &mut config.app.windows {
        if window.create {
            window.create = false;
            window.data_directory = Some(profile.clone());
            windows.push(window.clone());
        }
    }
    Ok(windows)
}

pub(crate) fn resolve_local_assets_root(
    executable: &Path,
    app_local_data: &Path,
) -> io::Result<PathBuf> {
    let executable_directory = executable
        .parent()
        .ok_or_else(|| io::Error::other("executable has no parent directory"))?;
    if executable_directory.join("portable.marker").is_file() {
        Ok(executable_directory.join("Chronicle-data"))
    } else {
        Ok(app_local_data.to_path_buf())
    }
}

pub(crate) fn local_assets_root(app: &AppHandle) -> Result<PathBuf, String> {
    resolve_local_assets_root(
        &std::env::current_exe().map_err(|error| error.to_string())?,
        &app.path()
            .app_local_data_dir()
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn resolve_repository_root(
    executable: &Path,
    app_local_data: &Path,
) -> io::Result<PathBuf> {
    let executable_directory = executable
        .parent()
        .ok_or_else(|| io::Error::other("executable has no parent directory"))?;

    if executable_directory.join("portable.marker").is_file() {
        Ok(executable_directory.join("Chronicle-data"))
    } else {
        Ok(app_local_data.join("Chronicle"))
    }
}
