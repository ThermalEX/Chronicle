use super::*;
use tempfile::tempdir;
const PNG: &[u8] = include_bytes!("../../src/assets/icon.png");
#[test]
fn native_path_icon_is_centered_and_rejects_invalid_images() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("wide.png");
    image::RgbaImage::from_pixel(40, 20, image::Rgba([200, 30, 40, 255]))
        .save(&path)
        .unwrap();
    let icon = native_icon_image_from_path(&path).unwrap();
    assert_eq!((icon.width(), icon.height()), (128, 128));
    assert_eq!(&icon.rgba()[0..4], &[0, 0, 0, 0]);
    let center = (64 * 128 + 64) * 4;
    assert_eq!(&icon.rgba()[center..center + 4], &[200, 30, 40, 255]);
    fs::write(&path, b"invalid").unwrap();
    assert!(native_icon_image_from_path(&path).is_err());
}
#[test]
fn saved_icon_is_local_and_independent_from_wallpaper() {
    let root = tempdir().unwrap();
    let source = root.path().join("selected.png");
    fs::write(&source, PNG).unwrap();
    let request = IconSaveRequest {
        mode: "image".into(),
        source_path: Some(source.to_string_lossy().into_owned()),
        remove_image: false,
    };
    save_icon_to(root.path(), request).unwrap();
    fs::remove_file(source).unwrap();
    let view = wallpaper::load_from(&root.path().join("app-icon")).unwrap();
    assert_eq!(view.mode, "image");
    assert!(view.image_data_url.is_some());
    assert_eq!(wallpaper::load_from(root.path()).unwrap().mode, "color");
}
#[test]
fn invalid_icon_cannot_replace_saved_copy() {
    let root = tempdir().unwrap();
    let source = root.path().join("selected.png");
    fs::write(&source, PNG).unwrap();
    save_icon_to(
        root.path(),
        IconSaveRequest {
            mode: "image".into(),
            source_path: Some(source.to_string_lossy().into_owned()),
            remove_image: false,
        },
    )
    .unwrap();
    fs::write(&source, b"fake").unwrap();
    assert!(
        save_icon_to(
            root.path(),
            IconSaveRequest {
                mode: "image".into(),
                source_path: Some(source.to_string_lossy().into_owned()),
                remove_image: false
            }
        )
        .is_err()
    );
    assert_eq!(
        wallpaper::load_from(&root.path().join("app-icon"))
            .unwrap()
            .mode,
        "image"
    );
}
#[test]
fn pack_import_validates_assets_and_does_not_save_preferences() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("icon.png"), PNG).unwrap();
    fs::write(root.path().join("background.png"), PNG).unwrap();
    let path = root.path().join("theme.json");
    fs::write(&path, br##"{"formatVersion":1,"customAccent":"#B83E49","colorMode":"dark","transparency":35,"blurPx":8,"icon":"icon.png","wallpaper":"background.png"}"##).unwrap();
    let view = preview_pack(&path).unwrap();
    assert_eq!(view.custom_accent, "#b83e49");
    assert!(view.icon_data_url.starts_with("data:image/png"));
    assert!(!root.path().join("wallpaper/preferences.json").exists());
    fs::remove_file(root.path().join("background.png")).unwrap();
    assert!(preview_pack(&path).is_err());
}
#[test]
fn theme_pack_rejects_traversal_and_invalid_colors() {
    let root = tempdir().unwrap();
    assert!(pack_asset(root.path(), "../outside.png").is_err());
    assert!(pack_asset(root.path(), "C:/outside.png").is_err());
    let path = root.path().join("theme.json");
    fs::write(&path, br##"{"formatVersion":1,"customAccent":"red;url(secret)","colorMode":"dark","transparency":35,"blurPx":8,"icon":"icon.png","wallpaper":"background.png"}"##).unwrap();
    assert!(preview_pack(&path).unwrap_err().contains("主题配置无效"));
}

#[test]
fn plain_theme_without_custom_images_can_be_imported() {
    let root = tempdir().unwrap();
    let path = root.path().join("theme.json");
    fs::write(&path, br##"{"formatVersion":1,"colorTheme":"teal","customAccent":"#B83E49","colorMode":"system","transparency":28,"blurPx":12}"##).unwrap();
    let view = preview_pack(&path).unwrap();
    assert!(view.icon_path.is_empty());
    assert!(view.wallpaper_path.is_empty());
}

#[test]
fn invalid_theme_sound_is_rejected_before_any_import_is_applied() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("bad.wav"), b"not a WAV").unwrap();
    let path = root.path().join("theme.json");
    fs::write(&path, br##"{"formatVersion":1,"colorTheme":"teal","customAccent":"#B83E49","colorMode":"system","transparency":28,"blurPx":12,"sounds":{"enabled":true,"volume":60,"files":{"notification":"bad.wav"}}}"##).unwrap();
    assert!(preview_pack(&path).is_err());
}

#[test]
fn sound_theme_round_trip_carries_unsaved_audio_without_persisting_it() {
    let local = tempdir().unwrap();
    let output = tempdir().unwrap();
    let path = local.path().join("通知.WAV");
    fs::write(&path, crate::local_sound::tests::wav()).unwrap();
    let mut request = export_request();
    request.sounds = Some(crate::local_sound::SoundSaveRequest {
        enabled: true,
        volume: 25,
        source_paths: [(
            "notification".into(),
            Some(path.to_string_lossy().into_owned()),
        )]
        .into(),
    });
    let exported = export_pack(local.path(), output.path(), request).unwrap();
    fs::remove_file(path).unwrap();
    let view = serde_json::to_value(preview_pack(&exported).unwrap()).unwrap();
    assert_eq!(view["sounds"]["enabled"], true);
    assert_eq!(view["sounds"]["volume"], 25);
    assert!(
        view["sounds"]["files"]["notification"]["dataUrl"]
            .as_str()
            .unwrap()
            .starts_with("data:audio/wav;base64,")
    );
    assert!(!local.path().join("theme-sounds").exists());
    let json = fs::read_to_string(&exported).unwrap();
    assert!(json.contains("sounds/notification.wav"));
    assert!(!json.contains("sourcePaths"));
    let manifest: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut request = export_request();
    request.sounds = Some(crate::local_sound::SoundSaveRequest {
        enabled: true,
        volume: 25,
        source_paths: [(
            "default".into(),
            Some(
                exported
                    .parent()
                    .unwrap()
                    .join("sounds/notification.wav")
                    .to_string_lossy()
                    .into_owned(),
            ),
        )]
        .into(),
    });
    let exported2 = export_pack(local.path(), output.path(), request).unwrap();
    assert_ne!(exported, exported2);
    let view = serde_json::to_value(preview_pack(&exported2).unwrap()).unwrap();
    assert!(view["sounds"]["files"].get("notification").is_none());
    assert!(view["sounds"]["files"].get("default").is_some());
    assert_eq!(
        manifest["sounds"]["files"]["notification"],
        "sounds/notification.wav"
    );
}

#[test]
fn sound_pack_rejects_unknown_events_and_paths_outside_the_pack() {
    let root = tempdir().unwrap();
    let path = root.path().join("theme.json");
    for (event, name) in [("notification", "../outside.wav"), ("secret", "sound.wav")] {
        let mut json: serde_json::Value = serde_json::from_str(r##"{"formatVersion":1,"colorTheme":"teal","customAccent":"#B83E49","colorMode":"system","transparency":28,"blurPx":12}"##).unwrap();
        json["sounds"] =
            serde_json::json!({ "enabled": true, "volume": 60, "files": { event: name } });
        fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(preview_pack(&path).is_err());
    }
}

fn export_request() -> ThemeExportRequest {
    serde_json::from_str(r##"{"colorTheme":"custom","customAccent":"#B83E49","colorMode":"dark","transparency":35,"blurPx":8,"iconMode":"color","wallpaperMode":"color"}"##).unwrap()
}

#[test]
#[ignore = "Set CHRONICLE_TEST_THEME_MANIFEST to a locally supplied theme; personal assets are not distributed"]
fn validate_local_theme_fixture() {
    let path = std::env::var("CHRONICLE_TEST_THEME_MANIFEST").unwrap();
    let view = preview_pack(Path::new(&path)).unwrap();
    let sounds = view.sounds.unwrap();
    assert_eq!(sounds.files.len(), local_sound::EVENTS.len());
    assert!(
        sounds
            .files
            .values()
            .all(|asset| asset.data_url.starts_with("data:audio/wav;base64,"))
    );
}

#[test]
fn export_carries_draft_assets_without_saving_preferences_or_absolute_paths() {
    let local = tempdir().unwrap();
    let output = tempdir().unwrap();
    let selected = local.path().join("unsaved.png");
    fs::write(&selected, PNG).unwrap();
    let mut request = export_request();
    request.icon_mode = "image".into();
    request.wallpaper_mode = "image".into();
    request.icon_source_path = Some(selected.to_string_lossy().into_owned());
    request.wallpaper_source_path = request.icon_source_path.clone();
    let path = export_pack(local.path(), output.path(), request).unwrap();
    fs::remove_file(selected).unwrap();
    let view = preview_pack(&path).unwrap();
    assert_eq!(view.color_theme, "custom");
    assert_eq!(view.custom_accent, "#b83e49");
    assert_eq!(view.color_mode, "dark");
    assert_eq!(view.transparency, 35);
    assert_eq!(view.blur_px, 8);
    assert!(view.icon_data_url.starts_with("data:image/png"));
    assert!(view.wallpaper_data_url.starts_with("data:image/png"));
    assert_eq!(fs::read_dir(local.path()).unwrap().count(), 0);
    let json = fs::read_to_string(path).unwrap();
    assert!(!json.contains(&local.path().to_string_lossy().to_string()));
    assert!(!json.contains("sourcePath"));
    assert!(!json.contains("device"));
    assert!(!json.contains("cloud"));
}

#[test]
fn export_preserves_preset_and_default_images_without_overwriting_previous_exports() {
    let local = tempdir().unwrap();
    let output = tempdir().unwrap();
    let mut request = export_request();
    request.color_theme = "teal".into();
    request.color_mode = "system".into();
    let first = export_pack(local.path(), output.path(), request).unwrap();
    let bytes = fs::read(&first).unwrap();
    let second = export_pack(local.path(), output.path(), export_request()).unwrap();
    assert_ne!(first, second);
    assert_eq!(fs::read(&first).unwrap(), bytes);
    let view = preview_pack(&first).unwrap();
    assert_eq!(view.color_theme, "teal");
    assert_eq!(view.color_mode, "system");
    assert!(view.icon_path.is_empty());
    assert!(view.wallpaper_path.is_empty());
    assert_eq!(fs::read_dir(first.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn export_reads_the_saved_copy_when_no_new_image_is_selected() {
    let local = tempdir().unwrap();
    let output = tempdir().unwrap();
    let selected = local.path().join("selected.png");
    fs::write(&selected, PNG).unwrap();
    save_icon_to(
        local.path(),
        IconSaveRequest {
            mode: "image".into(),
            source_path: Some(selected.to_string_lossy().into_owned()),
            remove_image: false,
        },
    )
    .unwrap();
    wallpaper::save_to(
        local.path(),
        wallpaper::WallpaperSaveRequest {
            mode: "image".into(),
            transparency: 28,
            blur_px: 12,
            source_path: Some(selected.to_string_lossy().into_owned()),
            remove_image: false,
        },
    )
    .unwrap();
    fs::remove_file(selected).unwrap();
    let previous = wallpaper::load_from(local.path()).unwrap().image_data_url;
    let mut request = export_request();
    request.icon_mode = "image".into();
    request.wallpaper_mode = "image".into();
    let path = export_pack(local.path(), output.path(), request).unwrap();
    let view = preview_pack(&path).unwrap();
    assert_eq!(Some(view.wallpaper_data_url), previous);
    assert_eq!(
        wallpaper::load_from(local.path()).unwrap().image_data_url,
        previous
    );
}

#[test]
fn export_rejects_invalid_images_or_options_before_creating_output() {
    let local = tempdir().unwrap();
    let output = tempdir().unwrap();
    let selected = local.path().join("bad.png");
    fs::write(&selected, b"not an image").unwrap();
    let mut request = export_request();
    request.icon_mode = "image".into();
    request.icon_source_path = Some(selected.to_string_lossy().into_owned());
    assert!(export_pack(local.path(), output.path(), request).is_err());
    let mut request = export_request();
    request.custom_accent = "nothex".into();
    assert!(export_pack(local.path(), output.path(), request).is_err());
    let mut request = export_request();
    request.wallpaper_mode = "image".into();
    assert!(export_pack(local.path(), output.path(), request).is_err());
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
}
