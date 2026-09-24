use super::*;

#[test]
fn source_paths_match_repository_canonical_paths() {
    let f = Fixture::new();
    f.file("games/Game/game.exe", "");
    f.file("games/Game/save/slot.dat", "save");
    let result = f.scan();
    assert_eq!(
        result.games[0].sources[0].path,
        f.0.join("games/Game/save")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
    );
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("chronicle-gal-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn file(&self, path: &str, body: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    fn scan(&self) -> ScanResult {
        scan_tree(
            &self.0.join("games"),
            &[self.0.join("profile")],
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn recursively_finds_games_and_prunes_detected_game_roots() {
    let f = Fixture::new();
    for game in [
        "合集/中文游戏",
        "合集/更深/日本語ゲーム",
        "合集/中文游戏/bonus",
    ] {
        f.file(&format!("games/{game}/Game.exe"), "");
        f.file(&format!("games/{game}/data.xp3"), "");
        f.file(&format!("games/{game}/SaveData/slot.dat"), "save");
    }
    let result = f.scan();
    assert_eq!(result.games.len(), 2);
    assert!(
        result
            .games
            .iter()
            .all(|g| g.sources.len() == 1 && g.engine == "Kirikiri")
    );
    assert!(result.games.iter().all(|g| !g.name.contains("bonus")));
}

#[test]
fn game_without_existing_saves_is_visible_but_has_no_sources() {
    let f = Fixture::new();
    f.file("games/Game/Game.exe", "");
    f.file("games/Game/data.xp3", "");
    let result = f.scan();
    assert_eq!(result.games.len(), 1);
    assert!(result.games[0].sources.is_empty());
}

#[test]
fn ordinary_apps_and_loose_assets_are_not_games() {
    let f = Fixture::new();
    f.file("games/Editor/editor.exe", "");
    f.file("games/Editor/package.json", r#"{"name":"editor"}"#);
    f.file("games/Editor/Data/readme.txt", "");
    f.file("games/Assets/archive.xp3", "");
    assert!(f.scan().games.is_empty());
}

#[test]
fn matches_external_names_exactly_and_keeps_internal_saves() {
    let f = Fixture::new();
    f.file("games/MyGame/MyGame.exe", "");
    f.file("games/MyGame/save/one.sav", "");
    f.file("profile/Vendor/mygame/saves/two.sav", "");
    f.file("profile/MyGame Extended/save/three.sav", "");
    f.file("profile/Game/save/four.sav", "");
    let result = f.scan();
    let sources = &result.games[0].sources;
    assert_eq!(sources.len(), 2);
    assert!(sources.iter().any(|s| s.evidence == "name"));
    assert!(!sources.iter().any(|s| s.path.contains("Extended")));
}

#[test]
fn renpy_uses_literal_save_directory_without_executing_config() {
    let f = Fixture::new();
    f.file("games/Renpy/Renpy.exe", "");
    f.file("games/Renpy/renpy/__init__.py", "");
    f.file("games/Renpy/game/options.rpy", "# Game options\n\ndefine config.name = \"Example\"\n\ndefine config.save_directory = \"Test-123\"\n");
    f.file("games/Renpy/game/saves/1.save", "");
    f.file("profile/RenPy/Test-123/2.save", "");
    assert_eq!(f.scan().games[0].sources.len(), 2);
    f.file(
        "games/Renpy/game/options.rpy",
        "define config.save_directory = \"../../escape\"\n",
    );
    assert_eq!(f.scan().games[0].sources.len(), 1);
}

#[test]
fn rpg_maker_root_only_includes_save_files_not_game_data() {
    let f = Fixture::new();
    for (game, archive, save) in [
        ("XP", "Game.rgssad", "Save01.rxdata"),
        ("VX", "Game.rgss2a", "Save01.rvdata"),
        ("Ace", "Game.rgss3a", "Save01.rvdata2"),
    ] {
        f.file(&format!("games/{game}/Game.exe"), "");
        f.file(&format!("games/{game}/{archive}"), "");
        f.file(&format!("games/{game}/{save}"), "");
        f.file(&format!("games/{game}/Data/Actors.rvdata2"), "");
    }
    for game in f.scan().games {
        assert_eq!(game.sources.len(), 1);
        assert_eq!(game.sources[0].kind, "file");
        assert!(game.sources[0].path.contains("Save01"));
    }
}

#[test]
fn detects_mv_mz_and_wolf_with_specific_markers() {
    let f = Fixture::new();
    f.file("games/MV/Game.exe", "");
    f.file("games/MV/www/js/rpg_core.js", "");
    f.file("games/MV/www/data/System.json", "{}");
    f.file("games/MV/www/save/file1.rpgsave", "");
    f.file("games/MZ/Game.exe", "");
    f.file("games/MZ/js/rmmz_core.js", "");
    f.file("games/MZ/data/System.json", "{}");
    f.file("games/MZ/save/file1.rmmzsave", "");
    f.file("games/Wolf/Game.exe", "");
    f.file("games/Wolf/Data/BasicData/Game.dat", "");
    f.file("games/Wolf/Save/Save01.sav", "");
    let result = f.scan();
    assert_eq!(result.games.len(), 3);
    assert!(
        result
            .games
            .iter()
            .all(|g| g.sources.len() == 1 && g.engine != "Generic")
    );
}

#[test]
fn can_scan_game_root_itself_and_cancel_without_results() {
    let f = Fixture::new();
    f.file("games/Game.exe", "");
    f.file("games/save/slot.sav", "");
    assert_eq!(f.scan().games.len(), 1);
    assert!(scan_tree(&f.0.join("games"), &[], &AtomicBool::new(true), |_, _| {}).is_err());
}

#[test]
fn validates_missing_changed_and_duplicate_sources() {
    let f = Fixture::new();
    f.file("games/Game/Game.exe", "");
    f.file("games/Game/save/slot.sav", "");
    let sources = f.scan().games.remove(0).sources;
    assert!(invalid_sources(&sources).is_empty());
    fs::remove_dir_all(&sources[0].path).unwrap();
    assert_eq!(invalid_sources(&sources), vec![sources[0].path.clone()]);
}

#[test]
fn cache_roundtrip_and_corruption() {
    let f = Fixture::new();
    f.file("games/Game/Game.exe", "");
    f.file("games/Game/save/slot.sav", "");
    let path = f.0.join("cache/results.json");
    assert!(read_cache(&path).unwrap().is_none());
    save_cache(&path, &f.scan()).unwrap();
    assert_eq!(read_cache(&path).unwrap().unwrap().games.len(), 1);
    fs::write(&path, "broken").unwrap();
    assert!(read_cache(&path).is_err());
}

#[test]
fn cancels_during_traversal_and_preserves_previous_cache() {
    let f = Fixture::new();
    f.file("games/One/Game.exe", "");
    f.file("games/One/save/slot.sav", "");
    f.file("games/Two/Game.exe", "");
    f.file("games/Two/save/slot.sav", "");
    let cache = f.0.join("cache/results.json");
    save_cache(&cache, &f.scan()).unwrap();
    let original = fs::read(&cache).unwrap();
    let cancelled = AtomicBool::new(false);
    let scan = scan_tree(&f.0.join("games"), &[], &cancelled, |checked, _| {
        if checked >= 2 {
            cancelled.store(true, Ordering::Relaxed);
        }
    });
    assert_eq!(scan.unwrap_err(), CANCELLED);
    assert_eq!(fs::read(&cache).unwrap(), original);
}

#[test]
fn matches_titles_in_config_and_ignores_expression_save_directories() {
    let f = Fixture::new();
    f.file("games/汉化/Game.exe", "");
    f.file("games/汉化/Game.rgss3a", "");
    f.file("games/汉化/Game.ini", "[Game]\nTitle=Original Title\n");
    f.file("profile/Vendor/Original Title/slot.sav", "");
    assert_eq!(f.scan().games[0].sources.len(), 1);
    f.file("games/Renpy/Renpy.exe", "");
    f.file("games/Renpy/game/archive.rpa", "");
    f.file(
        "games/Renpy/game/options.rpy",
        "define config.save_directory = \"Test-123\" + execute()\n",
    );
    f.file("profile/RenPy/Test-123/slot.save", "");
    assert!(
        f.scan()
            .games
            .iter()
            .find(|g| g.name == "Renpy")
            .unwrap()
            .sources
            .is_empty()
    );
}

#[test]
fn external_matching_never_adds_an_install_folder_or_its_parent() {
    let f = Fixture::new();
    f.file("profile/Collection/Title/Game.exe", "");
    f.file("profile/Collection/Title/Game.rgss3a", "");
    f.file("profile/Collection/Title/Save01.rvdata2", "");
    f.file("profile/Collection/Title/Game.ini", "Title=Collection\n");
    let result = scan_tree(
        &f.0.join("profile/Collection"),
        &[f.0.join("profile")],
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(result.games[0].sources.len(), 1);
    assert_eq!(result.games[0].sources[0].kind, "file");
}

#[test]
fn explicit_renpy_save_directory_can_be_short_or_generic() {
    let f = Fixture::new();
    f.file("games/Renpy/Renpy.exe", "");
    f.file("games/Renpy/game/archive.rpa", "");
    for name in ["ab", "game"] {
        f.file(
            "games/Renpy/game/options.rpy",
            &format!("define config.save_directory = \"{name}\""),
        );
        f.file(&format!("profile/RenPy/{name}/slot.save"), "");
        assert_eq!(f.scan().games[0].sources.len(), 1);
    }
}

#[test]
fn saved_games_honors_relocation_and_falls_back_for_invalid_values() {
    let f = Fixture::new();
    let moved = f.0.join("OtherDrive").join("Saves");
    assert_eq!(
        saved_games_directory(
            Some(moved.to_string_lossy().into_owned()),
            Some(f.0.clone())
        ),
        Some(moved)
    );
    assert_eq!(
        saved_games_directory(Some("relative/path".into()), Some(f.0.clone())),
        Some(f.0.join("Saved Games"))
    );
    assert_eq!(
        saved_games_directory(
            Some("%CHRONICLE_NONEXISTENT_LOCATION%/saves".into()),
            Some(f.0.clone())
        ),
        Some(f.0.join("Saved Games"))
    );
    #[cfg(windows)]
    assert_eq!(
        saved_games_directory(Some("%USERPROFILE%\\Saved Games".into()), None),
        dirs::home_dir().map(|p| p.join("Saved Games"))
    );
}

#[cfg(windows)]
#[test]
fn skips_junctions_in_scan_and_import_validation() {
    use std::os::windows::process::CommandExt;
    let f = Fixture::new();
    f.file("outside/Game.exe", "");
    f.file("outside/save/slot.sav", "");
    fs::create_dir_all(f.0.join("games")).unwrap();
    let link = f.0.join("games").join("Linked");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(f.0.join("outside"))
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "stdout: {} stderr: {}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(f.scan().games.is_empty());
    let source = FoundSource {
        path: link.join("save").to_string_lossy().into_owned(),
        kind: "folder".into(),
        evidence: "directory".into(),
    };
    assert_eq!(invalid_sources(&[source]).len(), 1);
    fs::remove_dir(&link).unwrap();
}
