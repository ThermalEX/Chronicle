//! Discover installed Steam games and resolve Ludusavi save paths locally.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";
type Manifest = BTreeMap<String, GameRules>;

#[derive(Default, Deserialize, Serialize)]
struct GameRules {
    #[serde(default)]
    files: BTreeMap<String, Rule>,
    #[serde(default)]
    registry: BTreeMap<String, Rule>,
    steam: Option<SteamId>,
    #[serde(default)]
    id: ExtraIds,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtraIds {
    #[serde(default)]
    steam_extra: Vec<u32>,
}
#[derive(Deserialize, Serialize)]
struct SteamId {
    id: u32,
}
#[derive(Default, Deserialize, Serialize)]
struct Rule {
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    when: Vec<Condition>,
}
#[derive(Deserialize, Serialize)]
struct Condition {
    os: Option<String>,
    store: Option<String>,
}
impl Rule {
    fn applicable(&self) -> bool {
        (self.tags.is_empty() || self.tags.iter().any(|tag| tag == "save"))
            && (self.when.is_empty()
                || self.when.iter().any(|c| {
                    c.os.as_deref().is_none_or(|os| os == "windows")
                        && c.store.as_deref().is_none_or(|s| s == "steam")
                }))
    }
}
#[derive(Deserialize, Serialize)]
struct Cache {
    updated_at: u64,
    etag: Option<String>,
    games: Manifest,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    games: Vec<FoundGame>,
    libraries: Vec<String>,
    warnings: Vec<String>,
    database_updated_at: u64,
    #[serde(default)]
    scanned_at: u64,
    #[serde(default)]
    steam_path: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct FoundGame {
    app_id: String,
    name: String,
    install_path: String,
    sources: Vec<FoundSource>,
    has_rules: bool,
}
#[derive(Deserialize, Serialize)]
struct FoundSource {
    path: String,
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user: Option<SteamUser>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SteamUser {
    account_id: String,
    display_name: String,
    steam_id64: Option<String>,
}

fn parse_steam_users(contents: &str) -> Result<BTreeMap<String, SteamUser>, String> {
    let vdf = parse_vdf(contents)?;
    let Some(Vdf::Object(users)) = vdf.get("users") else {
        return Err("Steam 用户配置缺少 users 节点".into());
    };
    let mut result = BTreeMap::new();
    for (id, data) in users {
        let Ok(id64) = id.parse::<u64>() else {
            continue;
        };
        // Public individual Steam accounts use this high 32-bit prefix.
        if id64 >> 32 != 0x0110_0001 {
            continue;
        }
        let account_id = (id64 & 0xffff_ffff).to_string();
        let display_name = ["PersonaName", "AccountName"]
            .iter()
            .filter_map(|key| data.get(key).and_then(Vdf::text))
            .map(str::trim)
            .find(|name| !name.is_empty())
            .map_or_else(|| format!("Steam 用户 {account_id}"), str::to_owned);
        result.insert(
            account_id.clone(),
            SteamUser {
                account_id,
                display_name,
                steam_id64: Some(id64.to_string()),
            },
        );
    }
    Ok(result)
}

fn load_steam_users(root: &Path, warnings: &mut Vec<String>) -> BTreeMap<String, SteamUser> {
    let contents = match fs::read_to_string(root.join("config/loginusers.vdf")) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return BTreeMap::new(),
        Err(_) => {
            warnings.push("Steam 用户配置无法读取，部分存档将显示用户 ID。".into());
            return BTreeMap::new();
        }
    };
    // Avoid exposing login configuration content in parse errors or persisted diagnostics.
    parse_steam_users(&contents).unwrap_or_else(|_| {
        warnings.push("Steam 用户配置格式无法识别，部分存档将显示用户 ID。".into());
        BTreeMap::new()
    })
}

fn source_user(
    source: &FoundSource,
    root: &Path,
    users: &BTreeMap<String, SteamUser>,
) -> Option<SteamUser> {
    if source.kind == "registry" {
        return None;
    }
    let path = source.path.replace('\\', "/").to_lowercase();
    let prefix = format!(
        "{}/userdata/",
        root.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    );
    if let Some(relative) = path.strip_prefix(&prefix) {
        let id = relative.split('/').next()?.parse::<u32>().ok()?.to_string();
        return Some(users.get(&id).cloned().unwrap_or_else(|| SteamUser {
            display_name: format!("Steam 用户 {id}"),
            account_id: id,
            steam_id64: None,
        }));
    }
    // Some games put a SteamID64 subdirectory in AppData; short numeric folders alone
    // are not sufficient evidence to attribute an otherwise local save to an account.
    users
        .values()
        .find(|user| {
            user.steam_id64
                .as_ref()
                .is_some_and(|id| path.split('/').any(|part| part == id))
        })
        .cloned()
}

// Steam's text KeyValues format has quoted strings, nested objects and // comments.
#[derive(Debug)]
enum Vdf {
    Text(String),
    Object(BTreeMap<String, Vdf>),
}
impl Vdf {
    fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(o) => o.get(key),
            Self::Text(_) => None,
        }
    }
    fn text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            Self::Object(_) => None,
        }
    }
}
fn parse_vdf(input: &str) -> Result<Vdf, String> {
    let mut chars = input.trim_start_matches('\u{feff}').chars().peekable();
    let mut tokens = Vec::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut value = String::new();
                let mut closed = false;
                while let Some(c) = chars.next() {
                    if c == '"' {
                        closed = true;
                        break;
                    }
                    if c == '\\' && chars.peek().is_some_and(|c| *c == '\\' || *c == '"') {
                        value.push(chars.next().unwrap());
                    } else {
                        value.push(c);
                    }
                }
                if !closed {
                    return Err("Steam 清单字符串不完整".into());
                }
                tokens.push((false, value));
            }
            '{' | '}' => tokens.push((true, c.to_string())),
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            _ => return Err("Steam 清单格式无法识别".into()),
        }
    }
    parse_vdf_object(&tokens, &mut 0, 0)
}

fn parse_vdf_object(
    tokens: &[(bool, String)],
    pos: &mut usize,
    depth: usize,
) -> Result<Vdf, String> {
    if depth > 32 {
        return Err("Steam 清单嵌套过深".into());
    }
    let mut out = BTreeMap::new();
    while *pos < tokens.len() {
        let (control, key) = &tokens[*pos];
        *pos += 1;
        if *control {
            if key == "}" && depth > 0 {
                return Ok(Vdf::Object(out));
            }
            return Err("Steam 清单括号不匹配".into());
        }
        let (control, value) = tokens.get(*pos).ok_or("Steam 清单缺少值")?;
        *pos += 1;
        let value = if *control {
            if value != "{" {
                return Err("Steam 清单缺少对象".into());
            }
            parse_vdf_object(tokens, pos, depth + 1)?
        } else {
            Vdf::Text(value.clone())
        };
        out.insert(key.clone(), value);
    }
    if depth > 0 {
        return Err("Steam 清单对象不完整".into());
    }
    Ok(Vdf::Object(out))
}

#[cfg(windows)]
fn steam_root() -> Option<PathBuf> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Valve\\Steam")
        .ok()
        .and_then(|key| key.get_value::<String, _>("SteamPath").ok())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("ProgramFiles(x86)").map(|p| PathBuf::from(p).join("Steam")))
}
#[cfg(not(windows))]
fn steam_root() -> Option<PathBuf> {
    None
}

fn libraries(root: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let mut result = vec![root.to_path_buf()];
    match fs::read_to_string(root.join("steamapps/libraryfolders.vdf"))
        .map_err(|e| e.to_string())
        .and_then(|s| parse_vdf(&s))
    {
        Ok(vdf) => {
            if let Some(Vdf::Object(items)) = vdf
                .get("libraryfolders")
                .or_else(|| vdf.get("LibraryFolders"))
            {
                for (id, value) in items {
                    if id.parse::<u32>().is_err() {
                        continue;
                    }
                    if let Some(path) = value
                        .get("path")
                        .and_then(Vdf::text)
                        .or_else(|| value.text())
                    {
                        let path = PathBuf::from(path);
                        if !result.iter().any(|p| {
                            p.to_string_lossy()
                                .eq_ignore_ascii_case(&path.to_string_lossy())
                        }) {
                            result.push(path);
                        }
                    }
                }
            }
        }
        Err(error) => warnings.push(format!("无法读取 Steam 多库配置，仅扫描指定目录：{error}")),
    }
    result
}

fn placeholders(
    root: &Path,
    library: &Path,
    install: &Path,
    app_id: &str,
) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    let mut put = |key: &str, value: Option<PathBuf>| {
        if let Some(v) = value {
            vars.insert(
                key.to_owned(),
                glob::Pattern::escape(&v.to_string_lossy().replace('\\', "/")),
            );
        }
    };
    put("root", Some(library.to_path_buf()));
    put("base", Some(install.to_path_buf()));
    put("home", dirs::home_dir());
    put("winDocuments", dirs::document_dir());
    put("winAppData", dirs::config_dir());
    put("winLocalAppData", dirs::data_local_dir());
    put(
        "winLocalAppDataLow",
        dirs::home_dir().map(|p| p.join("AppData/LocalLow")),
    );
    for (token, env) in [
        ("winPublic", "PUBLIC"),
        ("winProgramData", "PROGRAMDATA"),
        ("winDir", "WINDIR"),
    ] {
        put(token, std::env::var_os(env).map(PathBuf::from));
    }
    // Steam userdata lives under the client root, even for games installed in another library.
    put("steam", Some(root.to_path_buf()));
    vars.insert(
        "game".into(),
        glob::Pattern::escape(&install.file_name().unwrap_or_default().to_string_lossy()),
    );
    vars.insert("storeGameId".into(), app_id.into());
    vars.insert("storeUserId".into(), "*".into());
    if let Ok(name) = std::env::var("USERNAME") {
        vars.insert("osUserName".into(), glob::Pattern::escape(&name));
    }
    vars
}
fn resolve_pattern(pattern: &str, vars: &BTreeMap<String, String>) -> Option<String> {
    let mut result = pattern.replace('\\', "/");
    for (key, value) in vars {
        result = result.replace(&format!("<{key}>"), value);
    }
    if result.contains(['<', '>']) || !Path::new(&result).is_absolute() {
        return None;
    }
    Some(result)
}
fn find_paths(
    pattern: &str,
    sources: &mut BTreeMap<String, FoundSource>,
    warnings: &mut Vec<String>,
) {
    let Ok(paths) = glob::glob_with(
        pattern,
        glob::MatchOptions {
            case_sensitive: false,
            require_literal_separator: true,
            require_literal_leading_dot: false,
        },
    ) else {
        return;
    };
    for found in paths.take(5001) {
        if sources.len() >= 5000 {
            warnings.push("单个游戏匹配路径过多，已限制为 5000 个，请核对后添加。".into());
            break;
        }
        match found {
            Ok(path) => {
                // Exclude symbolic links from the selectable source list.
                if fs::symlink_metadata(&path).is_ok_and(|m| !m.file_type().is_symlink()) {
                    let kind = if path.is_file() {
                        "file"
                    } else if path.is_dir() {
                        "folder"
                    } else {
                        continue;
                    };
                    let text = path.to_string_lossy().into_owned();
                    sources.entry(text.to_lowercase()).or_insert(FoundSource {
                        path: text,
                        kind: kind.into(),
                        user: None,
                    });
                }
            }
            Err(error) => warnings.push(format!("部分路径无法访问：{error}")),
        }
    }
}
#[cfg(windows)]
fn registry_exists(path: &str) -> bool {
    use winreg::{
        RegKey,
        enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY},
    };
    let Some((hive, subkey)) = path.split_once('\\') else {
        return false;
    };
    let root = match hive {
        "HKEY_CURRENT_USER" => HKEY_CURRENT_USER,
        "HKEY_LOCAL_MACHINE" => HKEY_LOCAL_MACHINE,
        _ => return false,
    };
    !subkey.is_empty()
        && RegKey::predef(root)
            .open_subkey_with_flags(subkey, KEY_READ | KEY_WOW64_64KEY)
            .is_ok()
}
#[cfg(not(windows))]
fn registry_exists(_: &str) -> bool {
    false
}

fn scan(root: &Path, cache: &Cache, mut warnings: Vec<String>) -> Result<ScanResult, String> {
    if !root.join("steamapps").is_dir() {
        return Err("未找到 Steam 游戏库，请选择包含 steamapps 的 Steam 目录。".into());
    }
    let libraries = libraries(root, &mut warnings);
    let users = load_steam_users(root, &mut warnings);
    let index: BTreeMap<u32, &GameRules> = cache
        .games
        .values()
        .flat_map(|g| {
            g.steam
                .iter()
                .map(|s| s.id)
                .chain(g.id.steam_extra.iter().copied())
                .map(move |id| (id, g))
        })
        .collect();
    let mut games = BTreeMap::new();
    for library in &libraries {
        let entries = match fs::read_dir(library.join("steamapps")) {
            Ok(e) => e,
            Err(e) => {
                warnings.push(format!("无法读取游戏库 {}：{e}", library.display()));
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !entry
                .file_name()
                .to_string_lossy()
                .starts_with("appmanifest_")
                || path.extension().is_none_or(|e| e != "acf")
            {
                continue;
            }
            let parsed = fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|s| parse_vdf(&s));
            let vdf = match parsed {
                Ok(v) => v,
                Err(e) => {
                    warnings.push(format!("{}：{e}", path.display()));
                    continue;
                }
            };
            let Some(app) = vdf.get("AppState") else {
                continue;
            };
            let Some(id) = app
                .get("appid")
                .and_then(Vdf::text)
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            let Some(folder) = app.get("installdir").and_then(Vdf::text) else {
                continue;
            };
            if folder.contains(['/', '\\']) || folder == ".." {
                continue;
            }
            let install = library.join("steamapps/common").join(folder);
            if !install.is_dir() {
                continue;
            }
            let name = app
                .get("name")
                .and_then(Vdf::text)
                .unwrap_or(folder)
                .to_owned();
            let app_id = id.to_string();
            let vars = placeholders(root, library, &install, &app_id);
            let mut sources = BTreeMap::new();
            if let Some(rules) = index.get(&id) {
                for (pattern, rule) in &rules.files {
                    if rule.applicable()
                        && let Some(pattern) = resolve_pattern(pattern, &vars)
                    {
                        find_paths(&pattern, &mut sources, &mut warnings);
                    }
                }
                for (path, rule) in &rules.registry {
                    let path = path.replace('/', "\\");
                    if rule.applicable() && registry_exists(&path) {
                        sources.insert(
                            path.to_lowercase(),
                            FoundSource {
                                path,
                                kind: "registry".into(),
                                user: None,
                            },
                        );
                    }
                }
            }
            let userdata = format!("{}/userdata/*/{app_id}/remote", vars["steam"]);
            let mut cloud = BTreeMap::new();
            find_paths(&userdata, &mut cloud, &mut warnings);
            sources.extend(
                cloud
                    .into_iter()
                    .filter(|(_, s)| fs::read_dir(&s.path).is_ok_and(|mut d| d.next().is_some())),
            );
            let all_paths: Vec<_> = sources
                .values()
                .filter(|s| s.kind == "folder")
                .map(|s| {
                    s.path
                        .replace('\\', "/")
                        .trim_end_matches('/')
                        .to_lowercase()
                        + "/"
                })
                .collect();
            sources.retain(|_, s| {
                !all_paths
                    .iter()
                    .any(|p| s.path.replace('\\', "/").to_lowercase().starts_with(p))
            });
            for source in sources.values_mut() {
                source.user = source_user(source, root, &users);
            }
            games.entry(id).or_insert(FoundGame {
                app_id,
                name,
                install_path: install.to_string_lossy().into(),
                sources: sources.into_values().collect(),
                has_rules: index.get(&id).is_some_and(|rules| {
                    rules
                        .files
                        .values()
                        .chain(rules.registry.values())
                        .any(Rule::applicable)
                }),
            });
        }
    }
    let mut games: Vec<_> = games.into_values().collect();
    games.sort_by_key(|g| (g.sources.is_empty(), g.name.to_lowercase()));
    warnings.sort();
    warnings.dedup();
    warnings.truncate(30);
    Ok(ScanResult {
        games,
        libraries: libraries
            .iter()
            .map(|p| p.to_string_lossy().into())
            .collect(),
        warnings,
        database_updated_at: cache.updated_at,
        scanned_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        steam_path: root.to_string_lossy().into_owned(),
    })
}

async fn load_manifest(path: &Path, refresh: bool) -> Result<(Cache, Vec<String>), String> {
    let cached = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Cache>(&bytes).ok());
    if !refresh && let Some(cache) = cached {
        return Ok((cache, vec![]));
    }
    let download = async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .user_agent("Chronicle Steam save discovery")
            .build()
            .map_err(|e| e.to_string())?;
        let mut request = client.get(MANIFEST_URL);
        if let Some(etag) = cached.as_ref().and_then(|c| c.etag.as_ref()) {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let mut response = request.send().await.map_err(|e| e.to_string())?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        response = response.error_for_status().map_err(|e| e.to_string())?;
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|s| s.to_str().ok())
            .map(str::to_owned);
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            if bytes.len() + chunk.len() > 32 * 1024 * 1024 {
                return Err("路径数据库超过大小限制".to_owned());
            }
            bytes.extend_from_slice(&chunk);
        }
        let games: Manifest =
            serde_yaml_ng::from_slice(&bytes).map_err(|e| format!("路径数据库格式错误：{e}"))?;
        if games.is_empty() {
            return Err("路径数据库为空".into());
        }
        Ok(Some(Cache {
            games,
            etag,
            updated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }))
    }
    .await;
    match download {
        Ok(Some(cache)) => {
            let mut warnings = vec![];
            let save = (|| {
                fs::create_dir_all(path.parent().unwrap())?;
                fs::write(path, serde_json::to_vec(&cache)?)
            })();
            if let Err(e) = save {
                warnings.push(format!("本次扫描可用，但数据库缓存未保存：{e}"));
            }
            Ok((cache, warnings))
        }
        Ok(None) => cached
            .map(|c| (c, vec![]))
            .ok_or("数据库缓存不存在，请重试".into()),
        Err(e) => cached
            .map(|c| (c, vec![format!("更新失败，正在使用本机缓存：{e}")]))
            .ok_or(format!("首次扫描需要下载存档路径库，请联网后重试：{e}")),
    }
}

#[tauri::command]
pub async fn scan_steam_saves(
    state: tauri::State<'_, crate::AppState>,
    steam_path: Option<String>,
    refresh_database: bool,
) -> Result<ScanResult, String> {
    if !cfg!(windows) {
        return Err("Steam 存档扫描目前仅支持 Windows 桌面版。".into());
    }
    let root = steam_path
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .or_else(steam_root)
        .ok_or("未找到 Steam，请手动选择安装目录。")?;
    if !root.join("steamapps").is_dir() {
        return Err("未找到 Steam，请选择包含 steamapps 的安装目录。".into());
    }
    let path = state
        .repository
        .lock()
        .map_err(|e| e.to_string())?
        .root()
        .join("cache/steam-manifest.json");
    let (cache, warnings) = load_manifest(&path, refresh_database).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut result = scan(&root, &cache, warnings)?;
        let saved = save_scan_result(&path.with_file_name("steam-scan-results.json"), &result);
        if let Err(error) = saved {
            result
                .warnings
                .push(format!("扫描完成，但结果未保存：{error}"));
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn save_scan_result(path: &Path, result: &ScanResult) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("无效缓存目录")?).map_err(|e| e.to_string())?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    fs::write(
        &temporary,
        serde_json::to_vec(result).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&temporary, path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_steam_scan_results(
    state: tauri::State<'_, crate::AppState>,
) -> Result<Option<ScanResult>, String> {
    let path = state
        .repository
        .lock()
        .map_err(|e| e.to_string())?
        .root()
        .join("cache/steam-scan-results.json");
    read_scan_result(&path)
}

fn read_scan_result(path: &Path) -> Result<Option<ScanResult>, String> {
    match fs::read(path) {
        Ok(bytes) => {
            let mut result: ScanResult = serde_json::from_slice(&bytes)
                .map_err(|e| format!("上次扫描结果无法读取，请刷新扫描：{e}"))?;
            if !result.steam_path.is_empty() {
                let root = Path::new(&result.steam_path);
                let users = load_steam_users(root, &mut result.warnings);
                for source in result.games.iter_mut().flat_map(|game| &mut game.sources) {
                    if source.user.is_none() {
                        source.user = source_user(source, root, &users);
                    }
                }
            }
            Ok(Some(result))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("上次扫描结果无法读取：{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_sources_recover_accounts_without_rescanning() {
        let scope =
            std::env::temp_dir().join(format!("chronicle-account-cache-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(scope.join("config")).unwrap();
        fs::write(
            scope.join("config/loginusers.vdf"),
            r#""users" { "76561197960265851" { "PersonaName" "玩家甲" } }"#,
        )
        .unwrap();
        let saved = scope.join("results.json");
        let data = serde_json::json!({
            "games": [{"appId":"730", "name":"Counter-Strike 2", "installPath":"unused", "hasRules":false,
                "sources":[
                    {"path":scope.join("userdata/123/730/remote"),"kind":"folder"},
                    {"path":scope.join("userdata/408898812/730/remote"),"kind":"folder"},
                    {"path":scope.join("shared/save"),"kind":"folder"}
                ]}],
            "libraries":[], "warnings":[], "databaseUpdatedAt":1, "scannedAt":2, "steamPath":scope
        });
        fs::write(&saved, serde_json::to_vec(&data).unwrap()).unwrap();
        let result = read_scan_result(&saved).unwrap().unwrap();
        let sources = &result.games[0].sources;
        assert_eq!(
            sources[0].user.as_ref().map(|u| u.display_name.as_str()),
            Some("玩家甲")
        );
        assert_eq!(
            sources[1].user.as_ref().map(|u| u.account_id.as_str()),
            Some("408898812")
        );
        assert!(sources[2].user.is_none());
        assert_eq!(result.scanned_at, 2);
        fs::remove_dir_all(scope).unwrap();
    }
    #[test]
    fn steam_users_prefer_nickname_then_account_name_then_id() {
        let base = 76_561_197_960_265_728_u64;
        let data = format!(
            r#""users" {{
            "{}" {{ "PersonaName" "昵称 🐱" "AccountName" "login" "RememberPassword" "1" }}
            "{}" {{ "PersonaName" " " "AccountName" "fallback" }}
            "{}" {{ "AccountName" "" }}
            "bad" {{ "PersonaName" "invalid" }}
        }}"#,
            base + 123,
            base + 234,
            base + 4_000_000_000
        );
        let users = parse_steam_users(&data).unwrap();
        assert_eq!(users.len(), 3);
        assert_eq!(users["123"].display_name, "昵称 🐱");
        assert_eq!(users["234"].display_name, "fallback");
        assert_eq!(users["4000000000"].display_name, "Steam 用户 4000000000");
        let encoded = serde_json::to_string(&users).unwrap();
        assert!(!encoded.contains("RememberPassword"));
        assert!(!encoded.contains("login"));
        let source = |path: &str| FoundSource {
            path: path.into(),
            kind: "folder".into(),
            user: None,
        };
        let root = Path::new("D:/Steam");
        assert_eq!(
            source_user(
                &source("d:\\STEAM\\userdata\\123\\42\\remote"),
                root,
                &users
            )
            .unwrap()
            .display_name,
            "昵称 🐱"
        );
        assert_eq!(
            source_user(&source("D:/Steam/userdata/999/42/remote"), root, &users)
                .unwrap()
                .account_id,
            "999"
        );
        assert!(source_user(&source("D:/Else/userdata/123/save"), root, &users).is_none());
        assert!(source_user(&source("C:/Users/Me/AppData/Game/123/save"), root, &users).is_none());
        let id64_path = format!("C:/Users/Me/AppData/Game/{}/save", base + 123);
        assert_eq!(
            source_user(&source(&id64_path), root, &users)
                .unwrap()
                .account_id,
            "123"
        );
        let old: FoundSource =
            serde_json::from_str(r#"{"path":"D:/Saves","kind":"folder"}"#).unwrap();
        assert!(old.user.is_none());
    }
    #[test]
    #[ignore = "Downloads the public manifest and reads the local Steam installation"]
    fn live_manifest_and_local_steam() {
        let directory =
            std::env::temp_dir().join(format!("chronicle-steam-live-{}", uuid::Uuid::new_v4()));
        let cache_path = directory.join("manifest.json");
        let (cache, warnings) =
            tauri::async_runtime::block_on(load_manifest(&cache_path, true)).unwrap();
        assert!(cache.games.len() > 1000);
        println!("Parsed {} manifest entries", cache.games.len());
        if let Some(root) = steam_root().filter(|r| r.join("steamapps").is_dir()) {
            let result = scan(&root, &cache, warnings).unwrap();
            println!(
                "Scanned {} libraries, {} installed games, {} games with sources; warnings: {:?}",
                result.libraries.len(),
                result.games.len(),
                result
                    .games
                    .iter()
                    .filter(|g| !g.sources.is_empty())
                    .count(),
                result.warnings
            );
        }
        let (cached, _) =
            tauri::async_runtime::block_on(load_manifest(&cache_path, false)).unwrap();
        assert_eq!(cached.games.len(), cache.games.len());
        let (revalidated, _) =
            tauri::async_runtime::block_on(load_manifest(&cache_path, true)).unwrap();
        assert_eq!(revalidated.games.len(), cache.games.len());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn vdf_nested_escaped_paths_and_comments() {
        let parsed = parse_vdf(r#"// libraries
        "libraryfolders" { "0" { "path" "D:\\Steam" "apps" { "123" "42" } } "1" "E:\\SteamLibrary" }"#).unwrap();
        assert_eq!(
            parsed
                .get("libraryfolders")
                .unwrap()
                .get("0")
                .unwrap()
                .get("path")
                .unwrap()
                .text(),
            Some("D:\\Steam")
        );
        assert!(parse_vdf(r#""AppState" { "appid" "123""#).is_err());
    }
    #[test]
    fn only_windows_steam_save_rules_are_used() {
        for (yaml, expected) in [
            ("tags: [save]\nwhen: [{os: windows, store: steam}]", true),
            ("tags: [config]", false),
            ("when: [{os: windows, store: steam}]", true),
            ("tags: [save]\nwhen: [{os: linux}]", false),
            ("tags: [save]\nwhen: [{store: gog}]", false),
            ("tags: [save]\nwhen: [{os: linux}, {os: windows}]", true),
        ] {
            assert_eq!(
                serde_yaml_ng::from_str::<Rule>(yaml).unwrap().applicable(),
                expected
            );
        }
    }
    #[test]
    fn secondary_library_extra_id_and_literal_glob_characters() {
        let scope = std::env::temp_dir().join(format!("chronicle-steam-{}", uuid::Uuid::new_v4()));
        let root = scope.join("Steam");
        let library = scope.join("Library [2]");
        fs::create_dir_all(root.join("steamapps")).unwrap();
        fs::create_dir_all(library.join("steamapps/common/Game [test]/saves")).unwrap();
        fs::write(
            library.join("steamapps/common/Game [test]/saves/slot.sav"),
            "save",
        )
        .unwrap();
        let escaped = library.to_string_lossy().replace('\\', "\\\\");
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!("\"libraryfolders\" {{ \"1\" {{ \"path\" \"{escaped}\" }} }}"),
        )
        .unwrap();
        fs::write(
            library.join("steamapps/appmanifest_99.acf"),
            r#""AppState" { "appid" "99" "name" "Example" "installdir" "Game [test]" }"#,
        )
        .unwrap();
        let games = serde_yaml_ng::from_str("Example:\n  steam: {id: 42}\n  id: {steamExtra: [99]}\n  files:\n    '<base>/saves': {tags: [save]}\n    '<base>/saves/*.sav': {tags: [save]}").unwrap();
        let result = scan(
            &root,
            &Cache {
                games,
                etag: None,
                updated_at: 1,
            },
            vec![],
        )
        .unwrap();
        assert_eq!(result.libraries.len(), 2);
        assert_eq!(result.games.len(), 1);
        assert_eq!(
            result.games[0].sources.len(),
            1,
            "nested files must not duplicate their folder"
        );
        assert_eq!(result.games[0].sources[0].kind, "folder");
        assert!(result.warnings.is_empty());
        fs::remove_dir_all(scope).unwrap();
    }
    #[test]
    fn installed_games_resolve_actual_paths_and_userdata() {
        let root = std::env::temp_dir().join(format!("chronicle-steam-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("steamapps/common/Example/saves")).unwrap();
        fs::create_dir_all(root.join("userdata/123/42/remote")).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(
            root.join("config/loginusers.vdf"),
            r#""users" { "76561197960265851" { "PersonaName" "Example User" } }"#,
        )
        .unwrap();
        fs::write(root.join("userdata/123/42/remote/save.dat"), "save").unwrap();
        fs::write(
            root.join("steamapps/appmanifest_42.acf"),
            r#""AppState" { "appid" "42" "name" "Example" "installdir" "Example" }"#,
        )
        .unwrap();
        let games = serde_yaml_ng::from_str("Example:\n  steam: {id: 42}\n  files:\n    '<base>/saves': {tags: [save]}\n    '<base>/missing': {tags: [save]}\n    '<base>': {tags: [config]}").unwrap();
        let result = scan(
            &root,
            &Cache {
                games,
                etag: None,
                updated_at: 1,
            },
            vec![],
        )
        .unwrap();
        assert_eq!(result.games.len(), 1);
        assert_eq!(result.games[0].sources.len(), 2);
        let saved = root.join("cache/results.json");
        save_scan_result(&saved, &result).unwrap();
        save_scan_result(&saved, &result).unwrap();
        let restored: ScanResult = serde_json::from_slice(&fs::read(saved).unwrap()).unwrap();
        assert_eq!(restored.games[0].sources.len(), 2);
        assert_eq!(
            restored.games[0]
                .sources
                .iter()
                .filter_map(|s| s.user.as_ref())
                .next()
                .unwrap()
                .display_name,
            "Example User"
        );
        assert_eq!(restored.steam_path, result.steam_path);
        assert!(restored.scanned_at > 0);
        assert!(resolve_pattern("<unknown>/save", &BTreeMap::new()).is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
