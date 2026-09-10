mod auto_backup;
mod cloud;
mod commands;
mod storage_root;
#[cfg(test)]
mod storage_root_tests;

use std::{io, sync::{Arc, Mutex}};

use chronicle_storage::LocalRepository;
use tauri::{Emitter, Manager, WindowEvent, menu::{Menu, MenuItem}, tray::TrayIconBuilder};

pub(crate) struct AppState {
    pub repository: Arc<Mutex<LocalRepository>>,
    pub auto_backup: auto_backup::AutoBackupManager,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the Chronicle desktop application and its Tauri command runtime.
///
/// # Panics
/// Panics when the Tauri runtime cannot be started.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let executable = std::env::current_exe()?;
            let repository_root = storage_root::resolve_repository_root(
                &executable,
                &app.path().app_local_data_dir()?,
            )?;
            let repository = LocalRepository::open(repository_root)
                .map_err(|error| io::Error::other(error.to_string()))?;
            let repository = Arc::new(Mutex::new(repository));
            app.manage(AppState {
                auto_backup: auto_backup::AutoBackupManager::new(repository.clone(), app.handle().clone()),
                repository,
            });
            let show = MenuItem::with_id(app, "show", "显示 Chronicle", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "退出 Chronicle", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &exit])?;
            let tray_icon = app.default_window_icon().cloned()
                .ok_or_else(|| io::Error::other("Chronicle tray icon is missing"))?;
            TrayIconBuilder::with_id("main-tray").icon(tray_icon).menu(&menu).on_menu_event(|app, event| match event.id.as_ref() {
                "show" => { if let Some(window) = app.get_webview_window("main") { let _ = window.show(); let _ = window.set_focus(); } }
                "exit" => app.exit(0),
                _ => {}
            }).build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let behavior = window.state::<AppState>().repository.lock().ok().and_then(|repository| repository.load_settings().ok()).and_then(|settings| settings.pointer("/app/closeBehavior").and_then(serde_json::Value::as_str).map(str::to_owned)).unwrap_or_else(|| "ask".into());
                match behavior.as_str() {
                    "tray" => { api.prevent_close(); let _ = window.hide(); }
                    "ask" => { api.prevent_close(); let _ = window.emit("chronicle-close-requested", ()); }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_entries,
            commands::repository_info,
            commands::open_repository_folder,
            commands::open_entry_storage,
            commands::open_entry_sources,
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
            commands::hide_main_window,
            commands::exit_chronicle,
            commands::set_entry_automation,
            auto_backup::refresh_auto_backup,
            commands::update_snapshot_note,
            commands::delete_snapshot,
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
            cloud::save_opendal_credential,
            cloud::create_github_repository,
            cloud::cloud_source_statuses,
            cloud::test_cloud_source,
            cloud::cloud_preview,
            cloud::cloud_sync_entry,
            cloud::cloud_overwrite_upload,
            cloud::cloud_overwrite_download,
            cloud::cloud_upload_application_settings,
            cloud::cloud_upload_entry_category_tree,
            cloud::cloud_download_application_settings,
            cloud::cloud_delete_entries,
            cloud::cloud_delete_configurations,
            cloud::cloud_set_entry_sync_mode,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Chronicle");
}
