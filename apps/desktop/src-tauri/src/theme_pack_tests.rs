use super::*;
use serde_json::{Value, json};

fn pack_json() -> Value {
    json!({"formatVersion":2,"name":"牧濑红莉栖","colorTheme":"custom","customAccent":"#b83e49",
        "colorMode":"dark","transparency":35,"blurPx":8,"wallpapers":["wallpapers/a.jpg"],
        "selectedWallpaperIndex":0,"wallpaperPlayback":"fixed","intervalSeconds":60})
}
fn parse(value: &Value) -> Result<ThemePackV2, String> {
    parse_theme_config(&serde_json::to_vec(value).unwrap(), "旧主题")
}
pub(super) fn valid_draft() -> PersonalizationDraft {
    PersonalizationDraft {
        format_version: 1,
        mode: "custom".into(),
        solid: AppearanceFields::default(),
        selected_theme_id: Some("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()),
        themes: vec![ThemeDraft {
            id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),
            name: "牧濑红莉栖".into(),
            appearance: AppearanceFields {
                color_theme: "custom".into(),
                custom_accent: Some("#b83e49".into()),
                color_mode: "dark".into(),
            },
            transparency: 35,
            blur_px: 8,
            icon_source_path: None,
            wallpaper_source_paths: vec!["a.jpg".into()],
            selected_wallpaper_index: 0,
            wallpaper_playback: "fixed".into(),
            interval_seconds: 60,
            sounds: SoundSaveRequest::default(),
        }],
    }
}
#[test]
fn v2_uses_filename_name_and_defaults_empty_sounds() {
    let pack = parse(&pack_json()).unwrap();
    assert_eq!(pack.name, "旧主题");
    assert!(!pack.sounds.enabled);
    assert_eq!(pack.sounds.volume, 60);
    assert!(pack.sounds.files.is_empty());
}
#[test]
fn legacy_single_wallpaper_converts_without_random_playback() {
    let pack = parse(
        &json!({"formatVersion":1,"colorTheme":"custom","customAccent":"#b83e49",
        "colorMode":"dark","transparency":35,"blurPx":8,"wallpaper":"background.jpg"}),
    )
    .unwrap();
    assert_eq!(pack.name, "旧主题");
    assert_eq!(pack.wallpapers, vec!["background.jpg"]);
    assert_eq!(pack.wallpaper_playback, "fixed");
    assert_eq!(pack.interval_seconds, 60);
}
#[test]
fn valid_boundaries_and_unicode_names_are_accepted() {
    let mut value = pack_json();
    value["intervalSeconds"] = json!(86400);
    value["wallpapers"] = json!((0..32).map(|i| format!("{i}.png")).collect::<Vec<_>>());
    assert_eq!(
        parse_theme_config(
            &serde_json::to_vec(&value).unwrap(),
            &format!("  {}  ", "栖".repeat(64))
        )
        .unwrap()
        .name
        .chars()
        .count(),
        64
    );
    value["intervalSeconds"] = json!(1);
    value["transparency"] = json!(0);
    value["blurPx"] = json!(24);
    assert!(parse(&value).is_ok());
}
#[test]
fn invalid_configuration_is_rejected_without_panics() {
    for (key, bad) in [
        ("formatVersion", json!(3)),
        ("intervalSeconds", json!(0)),
        ("intervalSeconds", json!(86401)),
        ("intervalSeconds", json!(1.5)),
        ("transparency", json!(46)),
        ("blurPx", json!(25)),
        ("colorTheme", json!("other")),
        ("colorMode", json!("other")),
        ("customAccent", json!("红色主题")),
        ("selectedWallpaperIndex", json!(1)),
        ("wallpaperPlayback", json!("other")),
    ] {
        let mut value = pack_json();
        value[key] = bad;
        assert!(parse(&value).is_err(), "accepted {key}: {value}");
    }
    let mut value = pack_json();
    value["wallpapers"] = json!((0..33).map(|i| format!("{i}.png")).collect::<Vec<_>>());
    assert!(parse(&value).is_err());
    value["wallpapers"] = json!([]);
    assert!(parse(&value).is_ok());
    value["selectedWallpaperIndex"] = json!(1);
    assert!(parse(&value).is_err());
    assert!(parse_theme_config(&vec![b' '; 65537], "旧主题").is_err());
    for name in ["  ".to_string(), "栖".repeat(65)] {
        assert!(parse_theme_config(&serde_json::to_vec(&pack_json()).unwrap(), &name).is_err());
    }
}
#[test]
fn sound_event_keys_and_volume_are_validated() {
    let mut value = pack_json();
    value["sounds"] = json!({"enabled":true,"volume":60,"files":{"connected":"sounds/a.wav"}});
    assert!(parse(&value).is_ok());
    value["sounds"]["files"]["unknown"] = json!("x.wav");
    assert!(parse(&value).is_err());
    value["sounds"] = json!({"enabled":true,"volume":101,"files":{}});
    assert!(parse(&value).is_err());
}
#[test]
fn draft_identity_and_selection_are_validated() {
    let mut draft = valid_draft();
    assert!(validate_draft(&draft).is_ok());
    draft.themes.push(draft.themes[0].clone());
    assert!(validate_draft(&draft).is_err());
    draft.themes.pop();
    draft.selected_theme_id = Some("unknown".into());
    assert!(validate_draft(&draft).is_err());
    draft.mode = "solid".into();
    draft.selected_theme_id = None;
    assert!(validate_draft(&draft).is_ok());
    draft.themes[0].id = "../outside".into();
    assert!(validate_draft(&draft).is_err());
}
