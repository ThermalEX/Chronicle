mod commands;

use std::{io, sync::Mutex};

use chronicle_storage::LocalRepository;
use tauri::Manager;

pub(crate) struct AppState {
    pub repository: Mutex<LocalRepository>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let repository_root = app.path().app_local_data_dir()?.join("Chronicle");
            let repository = LocalRepository::open(repository_root)
                .map_err(|error| io::Error::other(error.to_string()))?;
            app.manage(AppState {
                repository: Mutex::new(repository),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_entries,
            commands::add_entry,
            commands::list_snapshots,
            commands::create_snapshot,
            commands::verify_snapshot,
            commands::restore_snapshot,
            commands::load_settings,
            commands::save_settings,
            commands::set_entry_category,
            commands::set_entry_tags,
            commands::list_categories,
            commands::create_category,
            commands::move_category,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Chronicle");
}
