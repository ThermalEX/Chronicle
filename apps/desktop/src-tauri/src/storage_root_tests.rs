use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::storage_root::{resolve_local_assets_root, resolve_repository_root};

#[test]
fn portable_webview_profile_is_local_and_distinct_from_other_copies() {
    let directory = tempfile::tempdir().unwrap();
    let mut profiles = Vec::new();
    for name in ["a", "b"] {
        let folder = directory.path().join(name);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("portable.marker"), "").unwrap();
        let mut config = tauri::Config::default();
        config.app.windows.push(Default::default());
        let windows = crate::storage_root::isolate_portable_webviews(
            &folder.join("Chronicle.exe"),
            &mut config,
        )
        .unwrap();
        assert_eq!(windows.len(), 1);
        assert!(!config.app.windows[0].create);
        let profile = config.app.windows[0].data_directory.clone().unwrap();
        assert_eq!(profile, folder.join("Chronicle-data/webview"));
        assert!(profile.is_dir());
        profiles.push(profile);
    }
    assert_ne!(profiles[0], profiles[1]);
}

#[test]
fn installed_webview_profile_preserves_the_existing_configuration() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = tauri::Config::default();
    config.app.windows.push(Default::default());
    config.app.windows.push(Default::default());
    config.app.windows[1].data_directory = Some(directory.path().join("existing-profile"));
    let windows = crate::storage_root::isolate_portable_webviews(
        &directory.path().join("Chronicle.exe"),
        &mut config,
    )
    .unwrap();
    assert!(windows.is_empty());
    assert!(config.app.windows[0].create);
    assert!(config.app.windows[0].data_directory.is_none());
    assert_eq!(
        config.app.windows[1].data_directory.as_ref().unwrap(),
        &directory.path().join("existing-profile")
    );
    assert!(!directory.path().join("Chronicle-data").exists());
}

fn test_directory(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("chronicle-{name}-{}", uuid::Uuid::new_v4()))
}

#[test]
fn portable_marker_uses_sibling_data_directory() {
    let directory = test_directory("portable-root");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("portable.marker"), "").unwrap();

    let result =
        resolve_repository_root(&directory.join("Chronicle.exe"), Path::new("C:/AppData")).unwrap();

    assert_eq!(result, directory.join("Chronicle-data"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn normal_installation_uses_app_local_data() {
    let directory = test_directory("installed-root");

    let result =
        resolve_repository_root(&directory.join("Chronicle.exe"), Path::new("C:/AppData")).unwrap();

    assert_eq!(result, PathBuf::from("C:/AppData").join("Chronicle"));
}

#[test]
fn portable_appearance_does_not_load_installed_wallpaper() {
    let directory = tempfile::tempdir().unwrap();
    let installed = directory.path().join("installed");
    fs::create_dir_all(installed.join("wallpaper")).unwrap();
    fs::write(
        installed.join("wallpaper/preferences.json"),
        r#"{"mode":"image","transparency":40,"blurPx":3,"imageFilename":null}"#,
    )
    .unwrap();
    fs::write(directory.path().join("portable.marker"), "").unwrap();

    let root =
        resolve_local_assets_root(&directory.path().join("Chronicle.exe"), &installed).unwrap();
    assert_eq!(root, directory.path().join("Chronicle-data"));
    let view = crate::wallpaper::load_from(&root).unwrap();
    let view = serde_json::to_value(view).unwrap();
    assert_eq!(view["transparency"], 28);
    assert_eq!(view["blurPx"], 12);
    assert_eq!(view["mode"], "color");
    assert!(installed.join("wallpaper/preferences.json").is_file());
}

#[test]
fn installed_appearance_preserves_existing_asset_location() {
    let directory = tempfile::tempdir().unwrap();
    let app_data = directory.path().join("app-data");
    let root =
        resolve_local_assets_root(&directory.path().join("Chronicle.exe"), &app_data).unwrap();
    assert_eq!(root, app_data);
}

#[test]
fn separate_portable_copies_do_not_share_appearance() {
    let directory = tempfile::tempdir().unwrap();
    let app_data = directory.path().join("app-data");
    let roots: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|name| {
            let folder = directory.path().join(name);
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("portable.marker"), "").unwrap();
            resolve_local_assets_root(&folder.join("Chronicle.exe"), &app_data).unwrap()
        })
        .collect();
    assert_ne!(roots[0], roots[1]);
}
