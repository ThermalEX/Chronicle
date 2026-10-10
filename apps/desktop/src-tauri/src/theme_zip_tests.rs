use super::*;
use std::io::{Cursor, Write};
use tempfile::tempdir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn three_local_packages_preserve_names_resources_and_exclude_absolute_paths() {
    let dir = tempdir().unwrap();
    for name in ["牧濑红莉栖", "粉蓝插画"] {
        let source = dir.path().join(name);
        std::fs::create_dir(&source).unwrap();
        let mut assets = theme_assets();
        let mut config: serde_json::Value = serde_json::from_slice(&assets["theme.json"]).unwrap();
        config["name"] = name.into();
        for event in [
            "connected",
            "disconnected",
            "connectionFailed",
            "notification",
            "default",
        ] {
            let path = format!("sounds/{event}.wav");
            config["sounds"]["files"][event] = path.clone().into();
            assets.insert(path, crate::local_sound::tests::wav());
        }
        assets.insert("theme.json".into(), serde_json::to_vec(&config).unwrap());
        for (path, bytes) in &assets {
            let target = source.join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, bytes).unwrap();
        }
        let output = dir.path().join(format!("{name}.zip"));
        crate::package_local_configuration(&source.join("theme.json"), &output).unwrap();
        let (pack, loaded) = read_theme_package(&output).unwrap();
        assert_eq!(pack.name, name);
        assert_eq!(pack.wallpapers.len(), 2);
        assert_eq!(pack.sounds.files.len(), 5);
        let json = String::from_utf8(loaded["theme.json"].clone()).unwrap();
        assert!(!json.contains(&dir.path().to_string_lossy().to_string()));
    }
    let source = dir.path().join("Rayburst");
    std::fs::create_dir(&source).unwrap();
    let mut files = serde_json::Map::new();
    for event in [
        "connected",
        "disconnected",
        "connectionFailed",
        "notification",
        "default",
    ] {
        let path = format!("{event}.wav");
        std::fs::write(source.join(&path), crate::local_sound::tests::wav()).unwrap();
        files.insert(event.into(), path.into());
    }
    std::fs::write(source.join("sounds.json"),serde_json::to_vec(&serde_json::json!({"formatVersion":1,"name":"Rayburst","sounds":{"enabled":true,"volume":60,"files":files}})).unwrap()).unwrap();
    let output = dir.path().join("Rayburst.zip");
    crate::package_local_configuration(&source.join("sounds.json"), &output).unwrap();
    let (pack, loaded) = read_sound_package(&output).unwrap();
    assert_eq!(pack.name.as_deref(), Some("Rayburst"));
    assert_eq!(pack.sounds.files.len(), 5);
    assert!(!loaded.contains_key("theme.json"));
    let defaults = include_str!("../tauri.conf.json");
    assert!(!defaults.contains("themes/"));
}

pub(crate) fn png() -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut output, image::ImageFormat::Png)
        .unwrap();
    output.into_inner()
}
fn zip_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn theme_assets() -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("theme.json".into(),serde_json::to_vec(&serde_json::json!({"formatVersion":2,"name":"牧濑红莉栖",
            "colorTheme":"custom","customAccent":"#b83e49","colorMode":"dark","transparency":35,"blurPx":8,
            "icon":"icon.png","wallpapers":["wallpapers/1.png","wallpapers/2.png"],"selectedWallpaperIndex":1,
            "wallpaperPlayback":"intervalRandom","intervalSeconds":17,
            "sounds":{"enabled":true,"volume":60,"files":{"connected":"sounds/connected.wav"}}})).unwrap()),
        ("icon.png".into(),png()),("wallpapers/1.png".into(),png()),("wallpapers/2.png".into(),png()),
        ("sounds/connected.wav".into(),crate::local_sound::tests::wav()),
    ])
}
#[test]
fn theme_roundtrip_preserves_resources_and_playback() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("牧濑红莉栖.zip");
    let assets = theme_assets();
    write_zip_assets(&path, &assets, false).unwrap();
    let (pack, loaded) = read_theme_package(&path).unwrap();
    assert_eq!(pack.name, "牧濑红莉栖");
    assert_eq!(
        pack.wallpapers,
        vec!["wallpapers/1.png", "wallpapers/2.png"]
    );
    assert_eq!(pack.selected_wallpaper_index, 1);
    assert_eq!(pack.interval_seconds, 17);
    assert_eq!(loaded, assets);
}
#[test]
fn zip_filename_overrides_legacy_json_name_and_supports_no_json_name() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("重命名主题.zip");
    let mut assets = theme_assets();
    write_zip_assets(&path, &assets, false).unwrap();
    assert_eq!(read_theme_package(&path).unwrap().0.name, "重命名主题");
    let mut config: serde_json::Value = serde_json::from_slice(&assets["theme.json"]).unwrap();
    config.as_object_mut().unwrap().remove("name");
    assets.insert("theme.json".into(), serde_json::to_vec(&config).unwrap());
    write_zip_assets(&path, &assets, true).unwrap();
    assert_eq!(read_theme_package(&path).unwrap().0.name, "重命名主题");
}
#[test]
fn unsafe_paths_are_rejected() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("unsafe.zip");
    for name in [
        "../x",
        "a/../x",
        "C:/x",
        "/x",
        "a\\..\\x",
        "a:stream",
        "CON",
        "x.",
        "x ",
        "https://a/x",
    ] {
        fs::write(&path, zip_bytes(&[(name, vec![1])])).unwrap();
        assert!(read_zip_assets(&path).is_err(), "accepted {name}");
    }
}
#[test]
fn case_collisions_duplicate_links_and_encryption_are_rejected() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("unsafe.zip");
    fs::write(&path, zip_bytes(&[("A.png", vec![1]), ("a.png", vec![2])])).unwrap();
    assert!(read_zip_assets(&path).is_err());
    let mut bytes = zip_bytes(&[("a.txt", vec![1]), ("b.txt", vec![2])]);
    for i in 0..bytes.len() - 5 {
        if &bytes[i..i + 5] == b"b.txt" {
            bytes[i] = b'a';
        }
    }
    fs::write(&path, bytes).unwrap();
    assert!(read_zip_assets(&path).is_err());
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink("link", "../outside", SimpleFileOptions::default())
        .unwrap();
    fs::write(&path, writer.finish().unwrap().into_inner()).unwrap();
    assert!(read_zip_assets(&path).is_err());
    let mut bytes = zip_bytes(&[("encrypted.txt", vec![1])]);
    bytes[6] |= 1;
    for i in 0..bytes.len() - 10 {
        if &bytes[i..i + 4] == b"PK\x01\x02" {
            bytes[i + 8] |= 1;
        }
    }
    fs::write(&path, bytes).unwrap();
    assert!(read_zip_assets(&path).is_err());
}
#[test]
fn missing_invalid_and_oversized_references_are_rejected() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bad.zip");
    let mut assets = theme_assets();
    assets.remove("icon.png");
    write_zip_assets(&path, &assets, false).unwrap();
    assert!(read_theme_package(&path).is_err());
    assets = theme_assets();
    assets.insert("icon.png".into(), b"not image".to_vec());
    write_zip_assets(&path, &assets, true).unwrap();
    assert!(read_theme_package(&path).is_err());
    assets = theme_assets();
    assets.insert("sounds/connected.wav".into(), b"not WAV".to_vec());
    write_zip_assets(&path, &assets, true).unwrap();
    assert!(read_theme_package(&path).is_err());
    assets = theme_assets();
    assets.insert("icon.png".into(), vec![0; 15 * 1024 * 1024 + 1]);
    write_zip_assets(&path, &assets, true).unwrap();
    assert!(read_theme_package(&path).unwrap_err().contains("15 MiB"));
    assets.insert("theme.json".into(), vec![b' '; 65537]);
    write_zip_assets(&path, &assets, true).unwrap();
    assert!(read_theme_package(&path).is_err());
}
#[test]
fn decompression_and_input_size_limits_are_enforced() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bomb.zip");
    let mut writer = ZipWriter::new(fs::File::create(&path).unwrap());
    writer
        .start_file(
            "bomb",
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
    let chunk = vec![0; 1024 * 1024];
    for _ in 0..257 {
        writer.write_all(&chunk).unwrap();
    }
    writer.finish().unwrap();
    assert!(read_zip_assets(&path).unwrap_err().contains("256 MiB"));
    let file = fs::File::create(&path).unwrap();
    file.set_len(256 * 1024 * 1024 + 1).unwrap();
    assert!(read_zip_assets(&path).unwrap_err().contains("256 MiB"));
}
#[test]
fn export_requires_overwrite_and_cleans_failed_temporaries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("theme.zip");
    fs::write(&path, b"original").unwrap();
    assert!(write_zip_assets(&path, &theme_assets(), false).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"original");
    let invalid = BTreeMap::from([("../outside".into(), vec![1])]);
    assert!(write_zip_assets(&path, &invalid, true).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"original");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn sound_package_and_theme_sound_only_import_share_validation() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sound.zip");
    let mut assets = theme_assets();
    write_zip_assets(&path, &assets, false).unwrap();
    let (sounds, _) = read_sound_package(&path).unwrap();
    assert_eq!(sounds.sounds.files.len(), 1);
    assets.remove("theme.json");
    assets.insert(
        "sounds.json".into(),
        serde_json::to_vec(&serde_json::json!({"formatVersion":1,
        "sounds":{"enabled":true,"volume":40,"files":{"connected":"sounds/connected.wav"}}}))
        .unwrap(),
    );
    write_zip_assets(&path, &assets, true).unwrap();
    let (pack, _) = read_sound_package(&path).unwrap();
    assert_eq!(pack.sounds.volume, 40);
}
#[test]
fn legacy_folder_import_enforces_reference_containment() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("theme.json");
    fs::write(dir.path().join("a.png"), png()).unwrap();
    let mut value = serde_json::json!({"formatVersion":1,"colorTheme":"teal","customAccent":"#b83e49",
        "colorMode":"dark","transparency":20,"blurPx":5,"wallpaper":"a.png"});
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(read_theme_package(&path).is_ok());
    value["wallpaper"] = serde_json::json!("../outside.png");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(read_theme_package(&path).is_err());
}
