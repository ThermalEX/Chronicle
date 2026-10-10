use super::*;
use tempfile::tempdir;

pub(crate) fn wav() -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&40_u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&44100_u32.to_le_bytes());
    bytes.extend_from_slice(&88200_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&4_u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes
}

fn request() -> SoundSaveRequest {
    SoundSaveRequest {
        enabled: true,
        volume: 40,
        source_paths: BTreeMap::new(),
    }
}

#[test]
fn wav_validation_rejects_fake_truncated_compressed_and_empty_audio() {
    let root = tempdir().unwrap();
    let path = root.path().join("tone.WAV");
    fs::write(&path, wav()).unwrap();
    assert!(sound_bytes(&path).is_ok());
    for invalid in [
        b"not WAV".to_vec(),
        wav()[..40].to_vec(),
        {
            let mut bytes = wav();
            bytes[20] = 2;
            bytes
        },
        {
            let mut bytes = wav();
            bytes[40..44].copy_from_slice(&0_u32.to_le_bytes());
            bytes
        },
    ] {
        fs::write(&path, invalid).unwrap();
        assert!(sound_bytes(&path).is_err());
    }
}

#[test]
fn sound_save_is_local_copied_and_survives_source_removal() {
    let root = tempdir().unwrap();
    let source = root.path().join("用户音效.WAV");
    fs::write(&source, wav()).unwrap();
    let mut save = request();
    save.source_paths.insert(
        "connected".into(),
        Some(source.to_string_lossy().into_owned()),
    );
    save_sounds_to(root.path(), save).unwrap();
    fs::remove_file(source).unwrap();
    let loaded = load_sounds_from(root.path()).unwrap();
    assert!(loaded.enabled);
    assert_eq!(loaded.volume, 40);
    assert_eq!(loaded.files["connected"].name, "用户音效.WAV");
    assert!(
        loaded.files["connected"]
            .data_url
            .starts_with("data:audio/wav;base64,")
    );
    assert!(!root.path().join("config/settings.json").exists());
}

#[test]
fn invalid_replacement_does_not_change_any_saved_sound_or_volume() {
    let root = tempdir().unwrap();
    let source = root.path().join("tone.wav");
    fs::write(&source, wav()).unwrap();
    let mut save = request();
    save.source_paths.insert(
        "notification".into(),
        Some(source.to_string_lossy().into_owned()),
    );
    save_sounds_to(root.path(), save).unwrap();
    let before = fs::read(root.path().join("theme-sounds/preferences.json")).unwrap();
    fs::write(&source, b"broken").unwrap();
    let mut save = request();
    save.volume = 90;
    save.source_paths.insert(
        "notification".into(),
        Some(source.to_string_lossy().into_owned()),
    );
    assert!(save_sounds_to(root.path(), save).is_err());
    assert_eq!(
        fs::read(root.path().join("theme-sounds/preferences.json")).unwrap(),
        before
    );
    assert_eq!(load_sounds_from(root.path()).unwrap().volume, 40);
}

#[test]
fn volume_keys_and_stored_names_are_validated() {
    let root = tempdir().unwrap();
    let mut save = request();
    save.volume = 101;
    assert!(save_sounds_to(root.path(), save).is_err());
    let mut save = request();
    save.source_paths.insert("../secret".into(), None);
    assert!(save_sounds_to(root.path(), save).is_err());
    assert!(!valid_sound_filename("../outside.wav"));
    assert!(!valid_sound_filename("sound-secret:stream.wav"));
    assert!(!root.path().join("theme-sounds").exists());
}

#[test]
fn missing_saved_audio_can_be_disabled_without_replacing_preferences() {
    let root = tempdir().unwrap();
    let source = root.path().join("tone.wav");
    fs::write(&source, wav()).unwrap();
    let mut save = request();
    save.source_paths.insert(
        "default".into(),
        Some(source.to_string_lossy().into_owned()),
    );
    save_sounds_to(root.path(), save).unwrap();
    let prefs = read_preferences(root.path()).unwrap();
    fs::remove_file(
        root.path()
            .join("theme-sounds")
            .join(&prefs.files["default"].filename),
    )
    .unwrap();
    let loaded = load_sounds_from(root.path()).unwrap();
    assert!(loaded.files["default"].data_url.is_empty());
    assert!(loaded.warning.is_some());
    let mut save = request();
    save.enabled = false;
    assert!(!save_sounds_to(root.path(), save).unwrap().enabled);
}

#[test]
fn remove_disable_missing_and_interrupted_preferences_are_safe() {
    let root = tempdir().unwrap();
    assert!(!load_sounds_from(root.path()).unwrap().enabled);
    let source = root.path().join("tone.wav");
    fs::write(&source, wav()).unwrap();
    let mut save = request();
    save.source_paths.insert(
        "default".into(),
        Some(source.to_string_lossy().into_owned()),
    );
    save_sounds_to(root.path(), save).unwrap();
    let folder = root.path().join("theme-sounds");
    fs::rename(
        folder.join("preferences.json"),
        folder.join("preferences.json.backup"),
    )
    .unwrap();
    assert!(
        load_sounds_from(root.path())
            .unwrap()
            .files
            .contains_key("default")
    );
    let mut save = request();
    save.enabled = false;
    save.source_paths.insert("default".into(), None);
    save_sounds_to(root.path(), save).unwrap();
    assert!(load_sounds_from(root.path()).unwrap().files.is_empty());
    fs::write(folder.join("preferences.json"), b"broken").unwrap();
    let loaded = load_sounds_from(root.path()).unwrap();
    assert!(!loaded.enabled);
    assert!(loaded.warning.is_some());
}

#[test]
fn missing_sound_remains_identifiable_and_can_be_removed_individually() {
    let root = tempdir().unwrap();
    let source = root.path().join("notification.wav");
    fs::write(&source, wav()).unwrap();
    let mut save = request();
    save.source_paths.insert(
        "notification".into(),
        Some(source.to_string_lossy().into_owned()),
    );
    save_sounds_to(root.path(), save).unwrap();
    let preferences = read_preferences(root.path()).unwrap();
    fs::remove_file(
        root.path()
            .join("theme-sounds")
            .join(&preferences.files["notification"].filename),
    )
    .unwrap();
    let loaded = load_sounds_from(root.path()).unwrap();
    assert_eq!(loaded.files["notification"].name, "notification.wav");
    assert!(loaded.files["notification"].data_url.is_empty());
    assert!(loaded.warning.is_some());
    let mut save = request();
    save.source_paths.insert("notification".into(), None);
    let loaded = save_sounds_to(root.path(), save).unwrap();
    assert!(loaded.files.is_empty());
    assert!(loaded.warning.is_none());
    assert!(
        resolved_sound_bytes(root.path(), &request())
            .unwrap()
            .is_empty()
    );
}
