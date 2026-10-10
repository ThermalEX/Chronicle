use super::*;
use tempfile::tempdir;
fn draft(source: &Path) -> PersonalizationDraft {
    let id = Uuid::new_v4().to_string();
    PersonalizationDraft {
        format_version: 1,
        mode: "custom".into(),
        solid: AppearanceFields::default(),
        selected_theme_id: Some(id.clone()),
        themes: vec![ThemeDraft {
            id,
            name: "牧濑红莉栖".into(),
            appearance: AppearanceFields::default(),
            transparency: 35,
            blur_px: 8,
            icon_source_path: None,
            wallpaper_source_paths: vec![source.to_string_lossy().into_owned()],
            selected_wallpaper_index: 0,
            wallpaper_playback: "fixed".into(),
            interval_seconds: 60,
            sounds: SoundSaveRequest::default(),
        }],
    }
}
#[test]
fn exported_theme_uses_zip_name_without_an_embedded_name_field() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.png");
    fs::write(&source, crate::theme_zip::tests::png()).unwrap();
    let mut theme = draft(&source).themes.remove(0);
    theme.name = "编辑后的名称".into();
    let output = dir.path().join("用户选择的文件名.zip");
    export_theme_package(output.to_string_lossy().into_owned(), theme, true).unwrap();
    let (pack, assets) = crate::theme_zip::read_theme_package(&output).unwrap();
    assert_eq!(pack.name, "用户选择的文件名");
    let config: serde_json::Value = serde_json::from_slice(&assets["theme.json"]).unwrap();
    assert!(config.get("name").is_none());
}
#[test]
fn saved_resources_survive_source_removal_and_portable_root_move() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.png");
    fs::write(&source, crate::theme_zip::tests::png()).unwrap();
    let root = dir.path().join("one");
    let saved = save_to(&root, &draft(&source)).unwrap();
    assert!(
        saved.warnings.is_empty(),
        "saved {saved:?}; index {}",
        fs::read_to_string(root.join("personalization/preferences.json")).unwrap()
    );
    fs::remove_file(&source).unwrap();
    let moved = dir.path().join("two");
    fs::rename(&root, &moved).unwrap();
    let loaded = load_from(&moved, &AppearanceFields::default()).unwrap();
    assert_eq!(loaded.draft.themes[0].id, saved.draft.themes[0].id);
    assert!(Path::new(&loaded.draft.themes[0].wallpaper_source_paths[0]).is_file());
    assert!(
        !fs::read_to_string(moved.join("personalization/preferences.json"))
            .unwrap()
            .contains("source.png")
    );
}
#[test]
fn failed_save_does_not_change_registry_or_original_files() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.png");
    let bytes = crate::theme_zip::tests::png();
    fs::write(&source, &bytes).unwrap();
    let root = dir.path().join("assets");
    let saved = save_to(&root, &draft(&source)).unwrap();
    let mut bad = saved.draft.clone();
    bad.themes[0].name = "未完成".into();
    bad.themes[0]
        .wallpaper_source_paths
        .push("missing.png".into());
    assert!(save_to(&root, &bad).is_err());
    assert_eq!(
        load_from(&root, &AppearanceFields::default()).unwrap(),
        saved
    );
    assert_eq!(fs::read(&source).unwrap(), bytes);
}
#[test]
fn legacy_loading_is_read_only_and_plain_appearance_stays_solid() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let plain = load_from(root, &AppearanceFields::default()).unwrap();
    assert_eq!(plain.draft.mode, "solid");
    assert!(plain.draft.themes.is_empty());
    fs::create_dir(root.join("wallpaper")).unwrap();
    let name = format!("wallpaper-{}.png", Uuid::new_v4());
    fs::write(
        root.join("wallpaper").join(&name),
        crate::theme_zip::tests::png(),
    )
    .unwrap();
    fs::write(
        root.join("wallpaper/preferences.json"),
        serde_json::to_vec(
            &serde_json::json!({"mode":"image","transparency":30,"blurPx":6,"imageFilename":name}),
        )
        .unwrap(),
    )
    .unwrap();
    let migrated = load_from(root, &AppearanceFields::default()).unwrap();
    assert_eq!(migrated.draft.mode, "custom");
    assert_eq!(migrated.draft.themes[0].name, "当前主题");
    assert!(!root.join("personalization/preferences.json").exists());
    assert!(root.join("wallpaper/preferences.json").exists());
}
#[test]
fn corrupt_registry_recovers_previous_saved_state_with_warning() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.png");
    fs::write(&source, crate::theme_zip::tests::png()).unwrap();
    let root = dir.path().join("assets");
    let first = save_to(&root, &draft(&source)).unwrap();
    let mut changed = first.draft.clone();
    changed.themes[0].name = "第二次".into();
    save_to(&root, &changed).unwrap();
    fs::write(root.join("personalization/preferences.json"), b"broken").unwrap();
    let loaded = load_from(&root, &AppearanceFields::default()).unwrap();
    assert_eq!(loaded.draft.themes[0].name, "牧濑红莉栖");
    assert!(!loaded.warnings.is_empty());
}
#[test]
fn stage_cleanup_rejects_non_uuid_and_leaves_other_files() {
    let dir = tempdir().unwrap();
    let outside = dir.path().join("outside");
    fs::write(&outside, b"keep").unwrap();
    assert!(discard_stages(dir.path(), &["../outside".into()]).is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"keep");
    let id = Uuid::new_v4().to_string();
    let folder = dir.path().join("personalization/imports").join(&id);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("own"), b"own").unwrap();
    discard_stages(dir.path(), &[id]).unwrap();
    assert!(!folder.exists());
    assert!(outside.exists());
}
#[test]
fn exporting_draft_includes_all_assets_without_saving() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.png");
    fs::write(&source, crate::theme_zip::tests::png()).unwrap();
    let theme = draft(&source).themes.remove(0);
    let assets = build_theme_assets(&theme).unwrap();
    let pack = crate::theme_pack::parse_theme_config(&assets["theme.json"], "fallback").unwrap();
    assert_eq!(pack.name, "fallback");
    assert_eq!(pack.wallpapers.len(), 1);
    assert_eq!(assets.len(), 2);
    assert!(!dir.path().join("personalization").exists());
}
#[test]
fn appearance_changes_reuse_saved_assets_without_creating_resource_versions() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.png");
    fs::write(&source, crate::theme_zip::tests::png()).unwrap();
    let root = dir.path().join("assets");
    let saved = save_to(&root, &draft(&source)).unwrap();
    let mut changed = saved.draft.clone();
    changed.themes[0].appearance.color_mode = "light".into();
    changed.themes[0].name = "重新命名".into();
    let updated = save_to(&root, &changed).unwrap();
    assert_eq!(
        updated.draft.themes[0].wallpaper_source_paths,
        saved.draft.themes[0].wallpaper_source_paths
    );
    assert_eq!(updated.draft.themes[0].appearance.color_mode, "light");
    assert_eq!(
        fs::read_dir(
            root.join("personalization/themes")
                .join(&changed.themes[0].id)
        )
        .unwrap()
        .count(),
        1
    );
}
#[test]
fn native_icon_cache_only_skips_successfully_applied_icons() {
    let mut cached = None;
    let icon = Some("saved-icon.png".to_string());
    let mut attempts = 0;
    assert!(
        apply_native_if_changed(&mut cached, icon.clone(), || {
            attempts += 1;
            Err("native-icon-unavailable".into())
        })
        .is_err()
    );
    assert_eq!(cached, None);
    apply_native_if_changed(&mut cached, icon.clone(), || {
        attempts += 1;
        Ok(())
    })
    .unwrap();
    apply_native_if_changed(&mut cached, icon, || {
        attempts += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(attempts, 2);
    apply_native_if_changed(&mut cached, None, || {
        attempts += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(attempts, 3);
}
#[test]
fn native_icon_load_and_save_are_ordered_with_their_application() {
    use std::sync::{Arc, mpsc};
    let cached = Arc::new(Mutex::new(None));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let loading_cache = Arc::clone(&cached);
    let startup = std::thread::spawn(move || {
        load_and_apply_native_icon(
            &loading_cache,
            || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(PersonalizationState {
                    draft: solid(AppearanceFields::default()),
                    warnings: vec![],
                })
            },
            |_| Ok(()),
        )
        .unwrap();
    });
    entered_rx.recv().unwrap();
    assert!(
        cached.try_lock().is_err(),
        "native read must share the apply/save lock"
    );
    release_tx.send(()).unwrap();
    startup.join().unwrap();
    let dir = tempdir().unwrap();
    let mut changed = draft(&dir.path().join("image.png"));
    changed.themes[0].icon_source_path = Some("new-icon.png".into());
    let mut applied = None;
    load_and_apply_native_icon(
        &cached,
        || {
            Ok(PersonalizationState {
                draft: changed,
                warnings: vec![],
            })
        },
        |icon| {
            applied = icon.map(str::to_owned);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(applied.as_deref(), Some("new-icon.png"));
    assert_eq!(*cached.lock().unwrap(), Some(Some("new-icon.png".into())));
}
#[test]
fn saved_wallpaper_loads_as_binary_and_unknown_files_still_require_validation() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.png");
    let bytes = crate::theme_zip::tests::png();
    fs::write(&source, &bytes).unwrap();
    let saved = save_to(dir.path(), &draft(&source)).unwrap();
    assert_eq!(
        crate::wallpaper::read_wallpaper_image(
            dir.path(),
            Path::new(&saved.draft.themes[0].wallpaper_source_paths[0])
        )
        .unwrap(),
        bytes
    );
    let fake = dir.path().join("fake.png");
    fs::write(&fake, b"not an image").unwrap();
    assert!(crate::wallpaper::read_wallpaper_image(dir.path(), &fake).is_err());
    fs::remove_file(&source).unwrap();
    assert!(crate::wallpaper::read_wallpaper_image(dir.path(), &source).is_err());
}
