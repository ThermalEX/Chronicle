//! Offline visual-novel save discovery. Game files are never executed or modified.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};

const SAVE_DIRS: &[&str] = &["savedata", "save", "saves"];
const CANCELLED: &str = "galgame-scan-cancelled";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    root_path: String,
    scanned_at: u64,
    games: Vec<FoundGame>,
    warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FoundGame {
    name: String,
    install_path: String,
    engine: String,
    sources: Vec<FoundSource>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FoundSource {
    path: String,
    kind: String,
    evidence: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    task_id: String,
    checked_directories: usize,
    found_games: usize,
}

struct Task {
    id: String,
    cancelled: Arc<AtomicBool>,
}
#[derive(Default)]
pub struct ScanTasks(Mutex<Option<Task>>);

fn check_cancel(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        Err(CANCELLED.into())
    } else {
        Ok(())
    }
}

fn safe_metadata(path: &Path) -> Option<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() {
        return None;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Reparse points (including junctions), offline and recall-on-access placeholders.
        if metadata.file_attributes() & (0x400 | 0x1000 | 0x40000 | 0x400000) != 0 {
            return None;
        }
    }
    Some(metadata)
}

fn safe_path(path: &Path) -> bool {
    path.is_absolute() && path.ancestors().all(|p| safe_metadata(p).is_some())
}

fn children(path: &Path, warnings: &mut Vec<String>) -> BTreeMap<String, PathBuf> {
    let mut result = BTreeMap::new();
    if !safe_metadata(path).is_some_and(|m| m.is_dir()) {
        return result;
    }
    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if safe_metadata(&path).is_some() {
                            result.insert(entry.file_name().to_string_lossy().to_lowercase(), path);
                        }
                    }
                    Err(error) => warnings.push(format!("{}: {error}", path.display())),
                }
            }
        }
        Err(error) => warnings.push(format!("{}: {error}", path.display())),
    }
    result
}

fn relative(base: &Path, parts: &[&str]) -> Option<PathBuf> {
    let mut path = base.to_owned();
    for part in parts {
        path = children(&path, &mut vec![]).remove(&part.to_lowercase())?;
    }
    Some(path)
}

fn text_file(path: &Path) -> Option<String> {
    if !safe_path(path) || safe_metadata(path)?.len() > 1024 * 1024 {
        return None;
    }
    let mut bytes = vec![];
    fs::File::open(path)
        .ok()?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    if bytes.starts_with(&[0xff, 0xfe]) {
        return Some(String::from_utf16_lossy(
            &bytes[2..]
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        ));
    }
    String::from_utf8(bytes).ok()
}

fn add_source(game: &mut FoundGame, path: PathBuf, evidence: &str) {
    let Some(metadata) = safe_metadata(&path) else {
        return;
    };
    if !metadata.is_dir() && !metadata.is_file() {
        return;
    }
    let Ok(path) = path.canonicalize() else {
        return;
    };
    let path = path.to_string_lossy().into_owned();
    if !game
        .sources
        .iter()
        .any(|s| s.path.eq_ignore_ascii_case(&path))
    {
        game.sources.push(FoundSource {
            path,
            kind: if metadata.is_dir() { "folder" } else { "file" }.into(),
            evidence: evidence.into(),
        });
    }
}

fn add_save_dirs(game: &mut FoundGame, base: &Path, evidence: &str) -> bool {
    let entries = children(base, &mut vec![]);
    let mut found = false;
    for name in SAVE_DIRS {
        if let Some(path) = entries.get(*name).filter(|p| p.is_dir()) {
            add_source(game, path.clone(), evidence);
            found = true;
        }
    }
    found
}

fn detect_game(path: &Path, entries: &BTreeMap<String, PathBuf>) -> Option<FoundGame> {
    let extension = |ext: &str| {
        entries
            .iter()
            .any(|(name, p)| name.ends_with(ext) && p.is_file())
    };
    if !extension(".exe") {
        return None;
    }
    let has_file = |parts: &[&str]| relative(path, parts).is_some_and(|p| p.is_file());
    let game_dir = entries.get("game").filter(|p| p.is_dir());
    let renpy = game_dir.is_some_and(|p| {
        let files = children(p, &mut vec![]);
        files.keys().any(|n| n.ends_with(".rpa"))
            || (entries.contains_key("renpy")
                && files
                    .keys()
                    .any(|n| n.ends_with(".rpy") || n.ends_with(".rpyc")))
    });
    let ini = entries
        .get("game.ini")
        .and_then(|p| text_file(p))
        .unwrap_or_default();
    let ini_lower = ini.to_lowercase();
    let mut data_base = path.to_owned();
    let (engine, save_extension) = if extension(".xp3") {
        ("Kirikiri", None)
    } else if renpy {
        data_base = game_dir?.clone();
        ("Ren’Py", None)
    } else if has_file(&["www", "js", "rpg_core.js"]) && has_file(&["www", "data", "System.json"]) {
        data_base = entries.get("www")?.clone();
        ("RPG Maker MV", None)
    } else if has_file(&["js", "rpg_core.js"]) && has_file(&["data", "System.json"]) {
        ("RPG Maker MV", None)
    } else if has_file(&["js", "rmmz_core.js"]) && has_file(&["data", "System.json"]) {
        ("RPG Maker MZ", None)
    } else if extension(".rgss3a") || (ini_lower.contains("rgss3") && entries.contains_key("data"))
    {
        ("RPG Maker VX Ace", Some(".rvdata2"))
    } else if extension(".rgss2a") || (ini_lower.contains("rgss2") && entries.contains_key("data"))
    {
        ("RPG Maker VX", Some(".rvdata"))
    } else if extension(".rgssad") || (ini_lower.contains("rgss1") && entries.contains_key("data"))
    {
        ("RPG Maker XP", Some(".rxdata"))
    } else if has_file(&["Data", "BasicData", "Game.dat"])
        || (has_file(&["Data.wolf"]) && has_file(&["Game.exe"]))
    {
        ("Wolf RPG", None)
    } else if SAVE_DIRS
        .iter()
        .any(|n| entries.get(*n).is_some_and(|p| p.is_dir()))
    {
        ("Generic", None)
    } else {
        return None;
    };
    let mut game = FoundGame {
        name: path.file_name()?.to_string_lossy().into_owned(),
        install_path: path.to_string_lossy().into_owned(),
        engine: engine.into(),
        sources: vec![],
    };
    add_save_dirs(
        &mut game,
        &data_base,
        if engine == "Generic" {
            "directory"
        } else {
            "engine"
        },
    );
    if let Some(ext) = save_extension {
        for (name, path) in entries {
            if name.ends_with(ext) && path.is_file() {
                add_source(&mut game, path.clone(), "engine");
            }
        }
    }
    Some(game)
}

fn usable_name(name: &str) -> bool {
    let name = name.to_lowercase();
    name.chars().count() > 2
        && ![
            "game",
            "start",
            "play",
            "setup",
            "config",
            "uninstall",
            "uninst",
            "bgi",
            "siglusengine",
            "advhd",
            "krkr",
            "boot",
            "launcher",
            "engine",
            "system",
            "renpy",
            "nw",
        ]
        .contains(&name.as_str())
        && !name.contains(['/', '\\', ':'])
        && name != ".."
}

fn candidate_names(game: &FoundGame) -> BTreeSet<String> {
    let entries = children(Path::new(&game.install_path), &mut vec![]);
    let mut names = vec![game.name.clone()];
    for (name, path) in &entries {
        if name.ends_with(".exe") && path.is_file() {
            names.push(name.trim_end_matches(".exe").to_owned());
        }
    }
    if let Some(text) = entries.get("game.ini").and_then(|p| text_file(p)) {
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim().eq_ignore_ascii_case("title") {
                    names.push(value.trim().into());
                }
            }
        }
    }
    if let Some(text) = entries.get("package.json").and_then(|p| text_file(p)) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
            for key in ["name", "productName"] {
                if let Some(name) = json.get(key).and_then(|v| v.as_str()) {
                    names.push(name.trim().into());
                }
            }
        }
    }
    names
        .into_iter()
        .filter(|n| usable_name(n))
        .map(|n| n.to_lowercase())
        .collect()
}

fn renpy_directory(game: &FoundGame) -> Option<String> {
    let path = relative(Path::new(&game.install_path), &["game", "options.rpy"])?;
    let text = text_file(&path)?;
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        if !["define config.save_directory", "config.save_directory"].contains(&key.trim()) {
            continue;
        }
        let value = value.trim();
        let quote = value.chars().next()?;
        if quote != '\'' && quote != '"' {
            continue;
        }
        let Some((name, tail)) = value[1..].split_once(quote) else {
            continue;
        };
        if !tail.trim().is_empty() && !tail.trim().starts_with('#') {
            continue;
        }
        if !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains(['/', '\\', ':'])
            && name.trim_end_matches(['.', ' ']) == name
        {
            return Some(name.into());
        }
    }
    None
}

fn scan_tree(
    root: &Path,
    external_roots: &[PathBuf],
    cancelled: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<ScanResult, String> {
    check_cancel(cancelled)?;
    if !safe_path(root) || !root.is_dir() {
        return Err("galgame-invalid-root".into());
    }
    fs::read_dir(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let mut result = ScanResult {
        root_path: root.to_string_lossy().into_owned(),
        scanned_at: 0,
        games: vec![],
        warnings: vec![],
    };
    let mut pending = vec![root.to_owned()];
    let mut checked = 0;
    while let Some(path) = pending.pop() {
        check_cancel(cancelled)?;
        let entries = children(&path, &mut result.warnings);
        checked += 1;
        if let Some(game) = detect_game(&path, &entries) {
            result.games.push(game);
        } else {
            pending.extend(entries.into_values().filter(|p| p.is_dir()));
        }
        progress(checked, result.games.len());
    }
    // Index each user directory and one vendor level once, instead of once per game.
    let names: BTreeSet<_> = result.games.iter().flat_map(candidate_names).collect();
    let mut external: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for base in external_roots {
        if names.is_empty() {
            break;
        }
        check_cancel(cancelled)?;
        if !safe_path(base) {
            continue;
        }
        let directories = children(base, &mut result.warnings);
        for (name, path) in directories {
            check_cancel(cancelled)?;
            if !path.is_dir() {
                continue;
            }
            if names.contains(&name) {
                external.entry(name).or_default().push(path.clone());
            }
            for (name, child) in children(&path, &mut result.warnings) {
                if child.is_dir() && names.contains(&name) {
                    external.entry(name).or_default().push(child);
                }
            }
            checked += 1;
            progress(checked, result.games.len());
        }
    }
    let installations: Vec<_> = result
        .games
        .iter()
        .map(|g| normalized_path(Path::new(&g.install_path)))
        .collect();
    let is_external = |path: &Path| {
        let candidate = normalized_path(path);
        !installations.iter().any(|install| {
            candidate == *install
                || candidate.starts_with(&format!("{install}/"))
                || install.starts_with(&format!("{candidate}/"))
        })
    };
    for game in &mut result.games {
        check_cancel(cancelled)?;
        for name in candidate_names(game) {
            for path in external.get(&name).into_iter().flatten() {
                if !is_external(path) {
                    continue;
                }
                if !add_save_dirs(game, path, "name") {
                    add_source(game, path.clone(), "name");
                }
            }
        }
        if game.engine == "Ren’Py" {
            if let Some(name) = renpy_directory(game) {
                for base in external_roots {
                    if safe_path(base) {
                        if let Some(path) = relative(base, &["RenPy", &name]) {
                            if is_external(&path) {
                                add_source(game, path, "engine");
                            }
                        }
                    }
                }
            }
        }
        game.sources.sort_by(|a, b| a.path.cmp(&b.path));
    }
    check_cancel(cancelled)?;
    result
        .games
        .sort_by(|a, b| a.install_path.cmp(&b.install_path));
    result.scanned_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    Ok(result)
}

fn invalid_sources(sources: &[FoundSource]) -> Vec<String> {
    sources
        .iter()
        .filter(|s| {
            let path = Path::new(&s.path);
            !safe_path(path)
                || !safe_metadata(path).is_some_and(|m| match s.kind.as_str() {
                    "file" => m.is_file() && fs::File::open(path).is_ok(),
                    "folder" => m.is_dir() && fs::read_dir(path).is_ok(),
                    _ => false,
                })
        })
        .map(|s| s.path.clone())
        .collect()
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

fn saved_games_directory(configured: Option<String>, home: Option<PathBuf>) -> Option<PathBuf> {
    let expanded = configured.and_then(|value| {
        let mut parts = value.split('%');
        let mut result = parts.next()?.to_owned();
        while let Some(variable) = parts.next() {
            let suffix = parts.next()?;
            result.push_str(&std::env::var(variable).ok()?);
            result.push_str(suffix);
        }
        let path = PathBuf::from(result);
        path.is_absolute().then_some(path)
    });
    expanded.or_else(|| home.map(|p| p.join("Saved Games")))
}

fn saved_games_root() -> Option<PathBuf> {
    // Read the registered Known Folder location, including user relocation.
    #[cfg(windows)]
    let configured = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders")
        .ok()
        .and_then(|key| {
            key.get_value::<String, _>("{4C5C32FF-BB9D-43B0-B5B4-2D72E54EAAA4}")
                .ok()
        });
    #[cfg(not(windows))]
    let configured = None;
    saved_games_directory(configured, dirs::home_dir())
}

fn save_cache(path: &Path, result: &ScanResult) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("galgame-invalid-cache")?).map_err(|e| e.to_string())?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let saved = (|| {
        fs::write(
            &temporary,
            serde_json::to_vec(result).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if saved.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    saved
}
fn read_cache(path: &Path) -> Result<Option<ScanResult>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| "galgame-cache-corrupt".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
fn cache_path(state: &crate::AppState) -> Result<PathBuf, String> {
    Ok(state
        .repository
        .lock()
        .map_err(|e| e.to_string())?
        .root()
        .join("cache/galgame-scan-results.json"))
}

#[tauri::command]
pub fn load_galgame_scan_results(
    state: tauri::State<'_, crate::AppState>,
) -> Result<Option<ScanResult>, String> {
    read_cache(&cache_path(&state)?)
}

#[tauri::command]
pub fn validate_galgame_sources(sources: Vec<FoundSource>) -> Vec<String> {
    invalid_sources(&sources)
}

#[tauri::command]
pub fn cancel_galgame_scan(
    tasks: tauri::State<'_, ScanTasks>,
    task_id: String,
) -> Result<(), String> {
    if let Some(task) = tasks
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .filter(|t| t.id == task_id)
    {
        task.cancelled.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn scan_galgame_saves(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
    tasks: tauri::State<'_, ScanTasks>,
    root_path: String,
    task_id: String,
) -> Result<ScanResult, String> {
    if !cfg!(windows) {
        return Err("galgame-windows-only".into());
    }
    uuid::Uuid::parse_str(&task_id).map_err(|e| e.to_string())?;
    let cache = cache_path(&state)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut current = tasks.0.lock().map_err(|e| e.to_string())?;
        if current.is_some() {
            return Err("galgame-scan-busy".into());
        }
        *current = Some(Task {
            id: task_id.clone(),
            cancelled: cancelled.clone(),
        });
    }
    let handle = app.clone();
    let id = task_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let roots: Vec<_> = [
            dirs::config_dir(),
            dirs::data_local_dir(),
            dirs::document_dir(),
            saved_games_root(),
        ]
        .into_iter()
        .flatten()
        .collect();
        let mut last = Instant::now() - Duration::from_secs(1);
        let mut result = scan_tree(
            Path::new(&root_path),
            &roots,
            &cancelled,
            |checked, found| {
                if last.elapsed() >= Duration::from_millis(100) {
                    let _ = handle.emit(
                        "galgame-scan-progress",
                        Progress {
                            task_id: id.clone(),
                            checked_directories: checked,
                            found_games: found,
                        },
                    );
                    last = Instant::now();
                }
            },
        )?;
        // Serialize completion with cancellation, so a cancelled run cannot replace the cache.
        let current = handle.state::<ScanTasks>();
        let guard = current.0.lock().map_err(|e| e.to_string())?;
        check_cancel(&cancelled)?;
        if let Err(error) = save_cache(&cache, &result) {
            result.warnings.push(error);
        }
        drop(guard);
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    let mut current = tasks.0.lock().map_err(|e| e.to_string())?;
    if current.as_ref().is_some_and(|t| t.id == task_id) {
        *current = None;
    }
    result
}

#[cfg(test)]
mod tests;
