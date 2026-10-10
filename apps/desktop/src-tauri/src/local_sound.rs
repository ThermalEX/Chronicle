use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tauri::AppHandle;
use uuid::Uuid;

pub(crate) const EVENTS: [&str; 5] = [
    "connected",
    "disconnected",
    "connectionFailed",
    "notification",
    "default",
];
const FOLDER: &str = "theme-sounds";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundAsset {
    pub(crate) name: String,
    pub(crate) data_url: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundView {
    pub(crate) enabled: bool,
    pub(crate) volume: u8,
    pub(crate) files: BTreeMap<String, SoundAsset>,
    pub(crate) warning: Option<String>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundSaveRequest {
    pub(crate) enabled: bool,
    pub(crate) volume: u8,
    #[serde(default)]
    pub(crate) source_paths: BTreeMap<String, Option<String>>,
}
#[derive(Clone, Deserialize, Serialize)]
struct SavedSound {
    filename: String,
    name: String,
}
#[derive(Default, Deserialize, Serialize)]
struct SoundPreferences {
    enabled: bool,
    volume: u8,
    files: BTreeMap<String, SavedSound>,
}

fn defaults() -> SoundPreferences {
    SoundPreferences {
        enabled: false,
        volume: 60,
        files: BTreeMap::new(),
    }
}
fn valid_sound_filename(name: &str) -> bool {
    name.strip_prefix("sound-")
        .and_then(|name| name.strip_suffix(".wav"))
        .is_some_and(|id| Uuid::parse_str(id).is_ok())
}
fn read_preferences(root: &Path) -> Result<SoundPreferences, String> {
    let folder = root.join(FOLDER);
    let path = folder.join("preferences.json");
    let path = if path.exists() {
        path
    } else {
        folder.join("preferences.json.backup")
    };
    if !path.exists() {
        return Ok(defaults());
    }
    let prefs: SoundPreferences =
        serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if prefs.volume > 100
        || prefs.files.iter().any(|(key, asset)| {
            !EVENTS.contains(&key.as_str()) || !valid_sound_filename(&asset.filename)
        })
    {
        return Err("音效设置无效".into());
    }
    Ok(prefs)
}

pub(crate) fn sound_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 5 * 1024 * 1024 {
        return Err("音效必须是小于或等于 5 MiB 的 WAV 文件".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    validate_sound_bytes(&bytes)?;
    Ok(bytes)
}

pub(crate) fn validate_sound_bytes(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > 5 * 1024 * 1024 {
        return Err("音效必须是小于或等于 5 MiB 的 WAV 文件".into());
    }
    let invalid = || "仅支持完整的 PCM WAV 音效（最长 30 秒）".to_string();
    if bytes.len() < 44 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(invalid());
    }
    let end = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8;
    if end != bytes.len() {
        return Err(invalid());
    }
    let mut offset = 12;
    let mut format = None;
    let mut data_len = None;
    while offset + 8 <= end {
        let chunk = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let start = offset + 8;
        let stop = start
            .checked_add(size)
            .filter(|stop| *stop <= end)
            .ok_or_else(invalid)?;
        if chunk == b"fmt " {
            if size < 16 || format.is_some() {
                return Err(invalid());
            }
            let u16_at =
                |i| u16::from_le_bytes(bytes[start + i..start + i + 2].try_into().unwrap());
            let u32_at =
                |i| u32::from_le_bytes(bytes[start + i..start + i + 4].try_into().unwrap());
            let (tag, channels, sample_rate, byte_rate, align, bits) = (
                u16_at(0),
                u16_at(2),
                u32_at(4),
                u32_at(8),
                u16_at(12),
                u16_at(14),
            );
            if tag != 1
                || !matches!(channels, 1 | 2)
                || !matches!(bits, 8 | 16 | 24 | 32)
                || !(8000..=192000).contains(&sample_rate)
                || align != channels * (bits / 8)
                || byte_rate != sample_rate * u32::from(align)
            {
                return Err(invalid());
            }
            format = Some((byte_rate, align));
        } else if chunk == b"data" {
            if data_len.is_some() {
                return Err(invalid());
            }
            data_len = Some(size);
        }
        offset = stop + size % 2;
    }
    let (rate, align) = format.ok_or_else(invalid)?;
    let length = data_len.ok_or_else(invalid)?;
    if offset != end
        || length == 0
        || length % usize::from(align) != 0
        || length as u64 > u64::from(rate) * 30
    {
        return Err(invalid());
    }
    Ok(())
}

pub(crate) fn preview_sound(path: &Path) -> Result<SoundAsset, String> {
    Ok(SoundAsset {
        name: path
            .file_name()
            .ok_or("音效文件名无效")?
            .to_string_lossy()
            .into_owned(),
        data_url: format!(
            "data:audio/wav;base64,{}",
            STANDARD.encode(sound_bytes(path)?)
        ),
    })
}
pub(crate) fn load_sounds_from(root: &Path) -> Result<SoundView, String> {
    let (prefs, mut warning) = match read_preferences(root) {
        Ok(prefs) => (prefs, None),
        Err(_) => (defaults(), Some("音效设置损坏，已关闭音效".into())),
    };
    let mut files = BTreeMap::new();
    for (key, saved) in prefs.files {
        match preview_sound(&root.join(FOLDER).join(saved.filename)) {
            Ok(mut asset) => {
                asset.name = saved.name;
                files.insert(key, asset);
            }
            Err(_) => {
                // Keep the event identifiable so a missing file can still be removed.
                files.insert(
                    key,
                    SoundAsset {
                        name: saved.name,
                        data_url: String::new(),
                    },
                );
                warning = Some("部分音效无法读取，请重新选择文件".into());
            }
        }
    }
    Ok(SoundView {
        enabled: prefs.enabled,
        volume: prefs.volume,
        files,
        warning,
    })
}

pub(crate) fn resolved_sound_bytes(
    root: &Path,
    request: &SoundSaveRequest,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if request.volume > 100
        || request
            .source_paths
            .keys()
            .any(|key| !EVENTS.contains(&key.as_str()))
    {
        return Err("音效设置无效".into());
    }
    let saved = read_preferences(root).unwrap_or_else(|_| defaults());
    let mut files = BTreeMap::new();
    for key in EVENTS {
        let path = match request.source_paths.get(key) {
            Some(Some(path)) => Some(PathBuf::from(path)),
            Some(None) => None,
            None => saved
                .files
                .get(key)
                .map(|asset| root.join(FOLDER).join(&asset.filename)),
        };
        if let Some(path) = path {
            files.insert(key.into(), sound_bytes(&path)?);
        }
    }
    Ok(files)
}
fn save_sounds_to(root: &Path, request: SoundSaveRequest) -> Result<SoundView, String> {
    if request.volume > 100
        || request
            .source_paths
            .keys()
            .any(|key| !EVENTS.contains(&key.as_str()))
    {
        return Err("音效设置无效".into());
    }
    // Unchanged missing audio must not prevent turning sounds off or removing it.
    let resolved: BTreeMap<_, _> = request
        .source_paths
        .iter()
        .filter_map(|(key, path)| path.as_ref().map(|path| (key, path)))
        .map(|(key, path)| sound_bytes(Path::new(path)).map(|bytes| (key.clone(), bytes)))
        .collect::<Result<_, _>>()?;
    let previous = read_preferences(root).unwrap_or_else(|_| defaults());
    let mut prefs = SoundPreferences {
        enabled: request.enabled,
        volume: request.volume,
        files: previous.files.clone(),
    };
    let folder = root.join(FOLDER);
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let mut staged = Vec::new();
    let mut commit = || -> Result<(), String> {
        for (key, path) in &request.source_paths {
            if let Some(path) = path {
                let filename = format!("sound-{}.wav", Uuid::new_v4());
                let target = folder.join(&filename);
                staged.push(target.clone());
                fs::write(&target, &resolved[key]).map_err(|e| e.to_string())?;
                prefs.files.insert(
                    key.clone(),
                    SavedSound {
                        filename,
                        name: Path::new(path)
                            .file_name()
                            .ok_or("音效文件名无效")?
                            .to_string_lossy()
                            .into_owned(),
                    },
                );
            } else {
                prefs.files.remove(key);
            }
        }
        crate::wallpaper::write_preferences(&folder.join("preferences.json"), &prefs)
    };
    if let Err(error) = commit() {
        for path in staged {
            let _ = fs::remove_file(path);
        }
        return Err(error);
    }
    for (key, old) in previous.files {
        if prefs
            .files
            .get(&key)
            .is_none_or(|asset| asset.filename != old.filename)
        {
            let _ = fs::remove_file(folder.join(old.filename));
        }
    }
    load_sounds_from(root)
}

#[tauri::command(async)]
pub fn load_local_sounds(app: AppHandle) -> Result<SoundView, String> {
    load_sounds_from(&crate::storage_root::local_assets_root(&app)?)
}
#[tauri::command(async)]
pub fn save_local_sounds(app: AppHandle, request: SoundSaveRequest) -> Result<SoundView, String> {
    save_sounds_to(&crate::storage_root::local_assets_root(&app)?, request)
}
#[tauri::command(async)]
pub fn preview_local_sound(path: String) -> Result<SoundAsset, String> {
    preview_sound(Path::new(&path))
}

#[cfg(test)]
#[path = "local_sound_tests.rs"]
pub(crate) mod tests;
