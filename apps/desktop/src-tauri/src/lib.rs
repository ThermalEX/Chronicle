mod auto_backup;
mod backup_automation;
mod backup_health;
mod cloud;
mod commands;
mod dropped_paths;
mod galgame_scan;
mod game_exit;
mod language;
mod local_icon;
mod local_sound;
mod onboarding;
mod process_monitor;
mod runtime_settings;
mod snapshot_sync;
mod steam_scan;
mod storage_root;
#[cfg(test)]
mod storage_root_tests;
mod theme_library;
mod theme_pack;
mod theme_zip;
mod update;
mod wallpaper;

use std::{
    io,
    sync::{Arc, Mutex},
};

use chronicle_storage::LocalRepository;
use tauri::{
    Emitter, Manager, WindowEvent,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_window_state::StateFlags;

/// Packages an explicitly selected local theme or sounds.json without changing settings.
pub fn package_local_configuration(
    config: &std::path::Path,
    output: &std::path::Path,
) -> Result<(), String> {
    theme_library::package_local_configuration(config, output)
}

pub(crate) struct AppState {
    pub repository: Arc<Mutex<LocalRepository>>,
    pub auto_backup: auto_backup::AutoBackupManager,
    pub close_behavior: runtime_settings::CloseBehavior,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the Chronicle desktop application and its Tauri command runtime.
///
/// # Panics
/// Panics when the Tauri runtime cannot be started.
pub fn run() {
    let mut context = tauri::generate_context!();
    let executable = std::env::current_exe().expect("failed to locate Chronicle executable");
    let portable_windows =
        storage_root::isolate_portable_webviews(&executable, context.config_mut())
            .expect("failed to isolate portable browser profile");
    tauri::Builder::default()
        .manage(galgame_scan::ScanTasks::default())
        .manage(backup_health::HealthTasks::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_filter(|label| label == "main")
                // A tray-hidden window should still be visible on the next launch.
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        .setup(move |app| {
            let executable = std::env::current_exe()?;
            let repository_root = storage_root::resolve_repository_root(
                &executable,
                &app.path().app_local_data_dir()?,
            )?;
            if let Err(error) = onboarding::initialize(&repository_root) {
                eprintln!("Tutorial state initialization failed: {error}");
            }
            let repository = LocalRepository::open(repository_root)
                .map_err(|error| io::Error::other(error.to_string()))?;
            let saved_settings = repository.load_settings().unwrap_or_default();
            let repository = Arc::new(Mutex::new(repository));
            app.manage(AppState {
                close_behavior: runtime_settings::CloseBehavior::new(&saved_settings),
                auto_backup: auto_backup::AutoBackupManager::new(
                    repository.clone(),
                    app.handle().clone(),
                ),
                repository,
            });
            for config in &portable_windows {
                tauri::WebviewWindowBuilder::from_config(app, config)?
                    .data_directory(
                        config
                            .data_directory
                            .clone()
                            .expect("portable browser profile"),
                    )
                    .build()?;
            }
            let (show_label, exit_label) = language::tray_labels(&saved_settings);
            let show = MenuItem::with_id(app, "show", show_label, true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", exit_label, true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &exit])?;
            app.manage(language::TrayMenu { show, exit });
            let tray_icon = app
                .default_window_icon()
                .cloned()
                .ok_or_else(|| io::Error::other("Chronicle tray icon is missing"))?;
            TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "exit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let behavior = window.state::<AppState>().close_behavior.get();
                match behavior {
                    "tray" => {
                        api.prevent_close();
                        if window.hide().is_ok() {
                            let _ = window.emit("chronicle-hidden-to-tray", ());
                        }
                    }
                    "ask" => {
                        api.prevent_close();
                        let _ = window.emit("chronicle-close-requested", ());
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            galgame_scan::scan_galgame_saves,
            galgame_scan::cancel_galgame_scan,
            galgame_scan::load_galgame_scan_results,
            galgame_scan::validate_galgame_sources,
            steam_scan::scan_steam_saves,
            steam_scan::load_steam_scan_results,
            onboarding::load_onboarding,
            onboarding::save_onboarding,
            commands::list_entries,
            commands::repository_info,
            snapshot_sync::preview_snapshot_sync,
            snapshot_sync::apply_snapshot_sync_plan,
            snapshot_sync::cancel_snapshot_sync,
            snapshot_sync::enable_snapshot_sync_protocol,
            snapshot_sync::cloud_upload_new_snapshots,
            snapshot_sync::read_known_devices,
            snapshot_sync::list_snapshot_recovery,
            snapshot_sync::read_remote_snapshot_recovery,
            snapshot_sync::restore_remote_snapshot_recovery,
            snapshot_sync::restore_snapshot_recovery,
            snapshot_sync::purge_snapshot_recovery,
            commands::read_device,
            commands::rename_device,
            commands::reset_device_identity,
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
            auto_backup::get_backup_runtime_states,
            backup_automation::get_backup_trigger,
            backup_automation::set_backup_trigger,
            process_monitor::list_backup_processes,
            commands::update_snapshot_note,
            commands::set_snapshot_locked,
            commands::delete_snapshot,
            commands::verify_snapshot,
            backup_health::start_backup_health_check,
            backup_health::cancel_backup_health_check,
            backup_health::get_backup_health_state,
            backup_health::load_backup_health_report,
            commands::restore_snapshot,
            update::fetch_release_feed,
            update::download_and_install_update,
            commands::load_settings,
            commands::save_settings,
            wallpaper::load_local_wallpaper,
            wallpaper::preview_local_wallpaper,
            wallpaper::save_local_wallpaper,
            local_icon::load_local_icon,
            local_icon::save_local_icon,
            local_icon::preview_local_theme,
            local_icon::export_local_theme,
            local_sound::load_local_sounds,
            local_sound::save_local_sounds,
            local_sound::preview_local_sound,
            theme_library::load_personalization,
            theme_library::apply_personalization_icon,
            theme_library::save_personalization,
            wallpaper::load_wallpaper_image,
            theme_library::preview_theme_package,
            theme_library::preview_theme_sound_package,
            theme_library::export_theme_package,
            theme_library::discard_theme_imports,
            theme_library::preview_theme_thumbnail,
            dropped_paths::describe_dropped_paths,
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
            cloud::cloud_upload_sync_metadata,
            cloud::cloud_upload_entry_category_tree,
            cloud::cloud_upload_entry_category_trees,
            cloud::cloud_download_application_settings,
            cloud::cloud_delete_entries,
            cloud::cloud_delete_configurations,
            cloud::cloud_set_entry_sync_mode,
        ])
        .run(context)
        .expect("failed to run Chronicle");
}
