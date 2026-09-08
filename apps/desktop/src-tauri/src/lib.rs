mod cloud;
mod commands;

use std::{io, sync::Mutex};

use chronicle_storage::LocalRepository;
use tauri::Manager;

pub(crate) struct AppState {
    pub repository: Mutex<LocalRepository>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the Chronicle desktop application and its Tauri command runtime.
///
/// # Panics
/// Panics when the Tauri runtime cannot be started.
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
            commands::repository_info,
            commands::open_repository_folder,
            commands::open_recycle_bin,
            commands::add_entry,
            commands::update_entry,
            commands::delete_entry,
            commands::delete_category,
            commands::list_recycle_items,
            commands::restore_recycle_item,
            commands::permanently_delete_recycle_item,
            commands::empty_recycle_bin,
            commands::list_snapshots,
            commands::create_snapshot,
            commands::verify_snapshot,
            commands::restore_snapshot,
            commands::load_settings,
            commands::save_settings,
            commands::append_diagnostic,
            commands::list_diagnostics,
            commands::clear_diagnostics,
            commands::set_entry_category,
            commands::set_entry_tags,
            commands::list_categories,
            commands::create_category,
            commands::move_category,
            cloud::save_cloud_credential,
            cloud::test_cloud_source,
            cloud::cloud_preview,
            cloud::cloud_sync_entry,
            cloud::cloud_overwrite_upload,
            cloud::cloud_overwrite_download,
            cloud::cloud_delete_entries,
            cloud::cloud_set_entry_sync_mode,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Chronicle");
}
