use crate::theme_pack::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::AppHandle;
use uuid::Uuid;
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PersonalizationState {
    pub draft: PersonalizationDraft,
    pub warnings: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedTheme {
    stage_id: String,
    theme: ThemeDraft,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedSounds {
    stage_id: String,
    sounds: SoundSaveRequest,
}
fn solid(appearance: AppearanceFields) -> PersonalizationDraft {
    PersonalizationDraft {
        format_version: 1,
        mode: "solid".into(),
        solid: appearance,
        selected_theme_id: None,
        themes: vec![],
    }
}
fn from_pack(pack: ThemePackV2, folder: &Path, id: String) -> ThemeDraft {
    let source = |name: &str| folder.join(name).to_string_lossy().into_owned();
    ThemeDraft {
        id,
        name: pack.name,
        appearance: pack.appearance,
        transparency: pack.transparency,
        blur_px: pack.blur_px,
        icon_source_path: pack.icon.as_deref().map(source),
        wallpaper_source_paths: pack.wallpapers.iter().map(|name| source(name)).collect(),
        selected_wallpaper_index: pack.selected_wallpaper_index,
        wallpaper_playback: pack.wallpaper_playback,
        interval_seconds: pack.interval_seconds,
        sounds: SoundSaveRequest {
            enabled: pack.sounds.enabled,
            volume: pack.sounds.volume,
            source_paths: pack
                .sounds
                .files
                .iter()
                .map(|(event, path)| (event.clone(), Some(source(path))))
                .collect(),
        },
    }
}
fn persisted_path(folder: &Path, id: &str, name: &str) -> Result<String, String> {
    crate::theme_zip::validate_asset_path(name)?;
    let parts: Vec<_> = name.split('/').collect();
    if parts.len() < 4
        || parts[0] != "themes"
        || parts[1] != id
        || Uuid::parse_str(parts[2]).is_err()
    {
        return Err("主题资源路径无效".into());
    }
    let path = folder.join(name);
    if path.exists()
        && !path
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(folder.canonicalize().map_err(|e| e.to_string())?)
    {
        return Err("主题资源路径无效".into());
    }
    Ok(path.to_string_lossy().into_owned())
}
fn resolve_index(folder: &Path, bytes: &[u8]) -> Result<PersonalizationDraft, String> {
    let mut draft: PersonalizationDraft =
        serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    validate_draft(&draft)?;
    for theme in &mut draft.themes {
        if let Some(name) = &theme.icon_source_path {
            theme.icon_source_path = Some(persisted_path(folder, &theme.id, name)?);
        }
        for name in &mut theme.wallpaper_source_paths {
            *name = persisted_path(folder, &theme.id, name)?;
        }
        for name in theme.sounds.source_paths.values_mut().flatten() {
            *name = persisted_path(folder, &theme.id, name)?;
        }
    }
    Ok(draft)
}
fn legacy_json(path: &Path) -> Result<Option<serde_json::Value>, String> {
    if !path.exists() {
        return Ok(None);
    }
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 65536 {
        return Err("旧外观配置损坏".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map(Some)
        .map_err(|e| e.to_string())
}
fn legacy_image(root: &Path) -> Result<Option<String>, String> {
    let Some(prefs) = legacy_json(&root.join("wallpaper/preferences.json"))? else {
        return Ok(None);
    };
    if prefs["mode"] != "image" {
        return Ok(None);
    }
    let name = prefs["imageFilename"].as_str().ok_or("旧外观图片缺失")?;
    crate::theme_zip::validate_asset_path(name)?;
    if name.contains('/') {
        return Err("旧外观图片路径无效".into());
    }
    let path = root.join("wallpaper").join(name);
    crate::wallpaper::preview_from_path(&path)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}
pub fn load_from(root: &Path, legacy: &AppearanceFields) -> Result<PersonalizationState, String> {
    let folder = root.join("personalization");
    let mut warnings = vec![];
    let mut damaged = false;
    for name in [
        "preferences.json",
        "preferences.json.backup",
        "preferences.last-good.json",
    ] {
        let path = folder.join(name);
        if !path.exists() {
            continue;
        }
        match fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| resolve_index(&folder, &bytes))
        {
            Ok(draft) => return Ok(PersonalizationState { draft, warnings }),
            Err(_) => {
                warnings.push("个性化配置损坏，已尝试恢复备份".into());
                damaged = true;
            }
        }
    }
    if damaged {
        return Ok(PersonalizationState {
            draft: solid(AppearanceFields::default()),
            warnings,
        });
    }
    let mut draft = solid(legacy.clone());
    let image = match legacy_image(root) {
        Ok(v) => v,
        Err(e) => {
            warnings.push(e);
            None
        }
    };
    let icon = match legacy_image(&root.join("app-icon")) {
        Ok(v) => v,
        Err(e) => {
            warnings.push(e);
            None
        }
    };
    let mut sounds = SoundSaveRequest::default();
    match legacy_json(&root.join("theme-sounds/preferences.json")) {
        Ok(Some(prefs)) => {
            sounds.enabled = prefs["enabled"].as_bool().unwrap_or(false);
            sounds.volume = prefs["volume"].as_u64().unwrap_or(60).min(100) as u8;
            if let Some(files) = prefs["files"].as_object() {
                for event in crate::local_sound::EVENTS {
                    if let Some(name) = files.get(event).and_then(|v| v["filename"].as_str()) {
                        if crate::theme_zip::validate_asset_path(name).is_err()
                            || name.contains('/')
                        {
                            warnings.push("旧音效路径无效".into());
                            continue;
                        }
                        let path = root.join("theme-sounds").join(name);
                        match crate::local_sound::sound_bytes(&path) {
                            Ok(_) => {
                                sounds.source_paths.insert(
                                    event.into(),
                                    Some(path.to_string_lossy().into_owned()),
                                );
                            }
                            Err(e) => warnings.push(e),
                        }
                    }
                }
            }
        }
        Err(e) => warnings.push(e),
        _ => {}
    }
    if image.is_some() || icon.is_some() || !sounds.source_paths.is_empty() {
        let prefs = legacy_json(&root.join("wallpaper/preferences.json"))
            .ok()
            .flatten()
            .unwrap_or_default();
        let id = Uuid::new_v4().to_string();
        draft.mode = "custom".into();
        draft.selected_theme_id = Some(id.clone());
        draft.themes.push(ThemeDraft {
            id,
            name: "当前主题".into(),
            appearance: legacy.clone(),
            transparency: prefs["transparency"].as_u64().unwrap_or(28).min(45) as u8,
            blur_px: prefs["blurPx"].as_u64().unwrap_or(12).min(24) as u8,
            icon_source_path: icon,
            wallpaper_source_paths: image.into_iter().collect(),
            selected_wallpaper_index: 0,
            wallpaper_playback: "fixed".into(),
            interval_seconds: 60,
            sounds,
        });
    }
    Ok(PersonalizationState { draft, warnings })
}
pub fn build_theme_assets(theme: &ThemeDraft) -> Result<BTreeMap<String, Vec<u8>>, String> {
    validate_draft(&PersonalizationDraft {
        format_version: 1,
        mode: "custom".into(),
        solid: AppearanceFields::default(),
        selected_theme_id: Some(theme.id.clone()),
        themes: vec![theme.clone()],
    })?;
    let mut assets = BTreeMap::new();
    let mut wallpapers = vec![];
    let mut total = 0_usize;
    let mut add = |name: String, bytes: Vec<u8>| -> Result<(), String> {
        total += bytes.len();
        if total > 256 * 1024 * 1024 {
            return Err("主题包超过 256 MiB".into());
        }
        assets.insert(name, bytes);
        Ok(())
    };
    for (index, path) in theme.wallpaper_source_paths.iter().enumerate() {
        let (bytes, ext) = crate::wallpaper::export_image(Path::new(""), Some(path))?;
        let name = format!("wallpapers/{:03}.{ext}", index + 1);
        add(name.clone(), bytes)?;
        wallpapers.push(name);
    }
    let icon = if let Some(path) = &theme.icon_source_path {
        let (bytes, ext) = crate::wallpaper::export_image(Path::new(""), Some(path))?;
        let name = format!("icon.{ext}");
        add(name.clone(), bytes)?;
        Some(name)
    } else {
        None
    };
    let mut files = BTreeMap::new();
    for (event, path) in &theme.sounds.source_paths {
        if let Some(path) = path {
            let name = format!("sounds/{event}.wav");
            add(
                name.clone(),
                crate::local_sound::sound_bytes(Path::new(path))?,
            )?;
            files.insert(event.clone(), name);
        }
    }
    let pack = ThemePackV2 {
        format_version: 2,
        name: theme.name.trim().into(),
        appearance: theme.appearance.clone(),
        transparency: theme.transparency,
        blur_px: theme.blur_px,
        icon,
        wallpapers,
        selected_wallpaper_index: theme.selected_wallpaper_index,
        wallpaper_playback: theme.wallpaper_playback.clone(),
        interval_seconds: theme.interval_seconds,
        sounds: ThemeSoundPack {
            enabled: theme.sounds.enabled,
            volume: theme.sounds.volume,
            files,
        },
    };
    let bytes = serde_json::to_vec_pretty(&pack).map_err(|e| e.to_string())?;
    crate::theme_pack::parse_theme_config(&bytes, &theme.name)?;
    assets.insert("theme.json".into(), bytes);
    Ok(assets)
}
pub fn package_local_configuration(config: &Path, output: &Path) -> Result<(), String> {
    let config = config.canonicalize().map_err(|e| e.to_string())?;
    let folder = config.parent().ok_or("主题路径无效")?;
    if config.file_name().is_some_and(|name| name == "sounds.json") {
        if fs::metadata(&config).map_err(|e| e.to_string())?.len() > 65536 {
            return Err("主题配置文件过大".into());
        }
        let sound: crate::theme_pack::SoundPackV1 =
            serde_json::from_slice(&fs::read(&config).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if sound.format_version != 1 {
            return Err("不支持的声音包格式版本".into());
        }
        let value = serde_json::json!({"formatVersion":2,"name":sound.name.as_deref().unwrap_or("Sound pack"),
            "colorTheme":"teal","colorMode":"system","transparency":28,"blurPx":12,"wallpapers":[],
            "selectedWallpaperIndex":0,"wallpaperPlayback":"fixed","intervalSeconds":60,"sounds":sound.sounds});
        let pack = crate::theme_pack::parse_theme_config(
            &serde_json::to_vec(&value).map_err(|e| e.to_string())?,
            "Sound pack",
        )?;
        for name in pack.sounds.files.values() {
            crate::theme_zip::validate_asset_path(name)?;
            let mut current = folder.to_path_buf();
            for component in Path::new(name).components() {
                current.push(component);
                if fs::symlink_metadata(&current)
                    .map_err(|e| e.to_string())?
                    .file_type()
                    .is_symlink()
                {
                    return Err("主题资源不支持链接".into());
                }
            }
            if !current
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(folder)
            {
                return Err("主题资源路径无效".into());
            }
        }
        let theme = from_pack(pack, folder, Uuid::new_v4().to_string());
        let mut assets = build_theme_assets(&theme)?;
        let generated = parse_theme_config(&assets.remove("theme.json").unwrap(), "Sound pack")?;
        assets.insert(
            "sounds.json".into(),
            serde_json::to_vec_pretty(&crate::theme_pack::SoundPackV1 {
                format_version: 1,
                name: sound.name,
                sounds: generated.sounds,
            })
            .map_err(|e| e.to_string())?,
        );
        crate::theme_zip::write_zip_assets(output, &assets, false)?;
        crate::theme_zip::read_sound_package(output)?;
    } else {
        let (pack, _) = crate::theme_zip::read_theme_package(&config)?;
        let assets = build_theme_assets(&from_pack(pack, folder, Uuid::new_v4().to_string()))?;
        crate::theme_zip::write_zip_assets(output, &assets, false)?;
        crate::theme_zip::read_theme_package(output)?;
    }
    Ok(())
}
fn write_assets(folder: &Path, assets: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    for (name, bytes) in assets {
        crate::theme_zip::validate_asset_path(name)?;
        let path = folder.join(name);
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        std::io::Write::write_all(&mut file, bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn save_to(
    root: &Path,
    request: &PersonalizationDraft,
) -> Result<PersonalizationState, String> {
    validate_draft(request)?;
    let folder = root.join("personalization");
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let folder = folder.canonicalize().map_err(|e| e.to_string())?;
    let previous = load_from(root, &request.solid)?;
    let mut persisted = request.clone();
    let mut created: Vec<PathBuf> = vec![];
    let result = (|| {
        for theme in &mut persisted.themes {
            // Saved assets are immutable. Metadata-only edits must not decode/copy every image again.
            if previous.draft.themes.iter().any(|old| {
                old.id == theme.id
                    && old.wallpaper_source_paths == theme.wallpaper_source_paths
                    && old.icon_source_path == theme.icon_source_path
                    && old.sounds.source_paths == theme.sounds.source_paths
            }) {
                let mut reusable = theme.clone();
                let mut valid = true;
                for path in reusable
                    .wallpaper_source_paths
                    .iter_mut()
                    .chain(reusable.icon_source_path.iter_mut())
                    .chain(reusable.sounds.source_paths.values_mut().flatten())
                {
                    let Some(relative) = Path::new(path).canonicalize().ok().and_then(|absolute| {
                        absolute.strip_prefix(&folder).ok().map(Path::to_path_buf)
                    }) else {
                        valid = false;
                        break;
                    };
                    let name = relative.to_string_lossy().replace('\\', "/");
                    if persisted_path(&folder, &theme.id, &name).is_err()
                        || !Path::new(path).is_file()
                    {
                        valid = false;
                        break;
                    }
                    *path = name;
                }
                if valid {
                    *theme = reusable;
                    continue;
                }
            }
            let assets = build_theme_assets(theme)?;
            let pack = parse_theme_config(&assets["theme.json"], &theme.name)?;
            let relative = format!("themes/{}/{}", theme.id, Uuid::new_v4());
            let target = folder.join(&relative);
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            created.push(target.clone());
            write_assets(&target, &assets)?;
            *theme = from_pack(pack, Path::new(&relative), theme.id.clone());
            for path in theme
                .wallpaper_source_paths
                .iter_mut()
                .chain(theme.icon_source_path.iter_mut())
                .chain(theme.sounds.source_paths.values_mut().flatten())
            {
                *path = path.replace('\\', "/");
            }
        }
        let path = folder.join("preferences.json");
        if path.exists()
            && resolve_index(&folder, &fs::read(&path).map_err(|e| e.to_string())?).is_ok()
        {
            fs::copy(&path, folder.join("preferences.last-good.json"))
                .map_err(|e| e.to_string())?;
        }
        crate::wallpaper::write_preferences(&path, &persisted)?;
        load_from(root, &request.solid)
    })();
    if result.is_err() {
        for path in created {
            let _ = fs::remove_dir_all(path);
        }
    }
    result
}
pub fn discard_stages(root: &Path, stage_ids: &[String]) -> Result<(), String> {
    let ids: Vec<_> = stage_ids
        .iter()
        .map(|id| Uuid::parse_str(id).map_err(|_| "导入暂存标识无效".to_string()))
        .collect::<Result<_, _>>()?;
    let folder = root.join("personalization/imports");
    for id in ids {
        let path = folder.join(id.to_string());
        if path.exists() {
            if fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
                || !path
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .starts_with(folder.canonicalize().map_err(|e| e.to_string())?)
            {
                return Err("导入暂存路径无效".into());
            }
            fs::remove_dir_all(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
fn stage(root: &Path, assets: &BTreeMap<String, Vec<u8>>) -> Result<(String, PathBuf), String> {
    let id = Uuid::new_v4().to_string();
    let folder = root.join("personalization/imports").join(&id);
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    if let Err(error) = write_assets(&folder, assets) {
        let _ = fs::remove_dir_all(&folder);
        return Err(error);
    }
    Ok((id, folder))
}
static APPLIED_NATIVE_ICON: Mutex<Option<Option<String>>> = Mutex::new(None);
fn apply_native_if_changed(
    cached: &mut Option<Option<String>>,
    icon: Option<String>,
    apply: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if cached.as_ref() == Some(&icon) {
        return Ok(());
    }
    *cached = None;
    apply()?;
    *cached = Some(icon);
    Ok(())
}
fn load_and_apply_native_icon(
    cache: &Mutex<Option<Option<String>>>,
    load: impl FnOnce() -> Result<PersonalizationState, String>,
    apply: impl FnOnce(Option<&str>) -> Result<(), String>,
) -> Result<PersonalizationState, String> {
    let mut cached = cache.lock().unwrap_or_else(|error| error.into_inner());
    let mut state = load()?;
    let icon = active_icon(&state.draft).map(str::to_owned);
    if let Err(warning) =
        apply_native_if_changed(&mut cached, icon.clone(), || apply(icon.as_deref()))
    {
        state.warnings.push(warning);
    }
    Ok(state)
}
fn active_icon(draft: &PersonalizationDraft) -> Option<&str> {
    if draft.mode == "custom" {
        draft
            .themes
            .iter()
            .find(|theme| Some(&theme.id) == draft.selected_theme_id.as_ref())
            .and_then(|theme| theme.icon_source_path.as_deref())
    } else {
        None
    }
}
#[tauri::command(async)]
pub fn load_personalization(
    app: AppHandle,
    legacy_appearance: AppearanceFields,
) -> Result<PersonalizationState, String> {
    load_from(
        &crate::storage_root::local_assets_root(&app)?,
        &legacy_appearance,
    )
}
#[tauri::command]
pub async fn apply_personalization_icon(app: AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = crate::storage_root::local_assets_root(&app)?;
        load_and_apply_native_icon(
            &APPLIED_NATIVE_ICON,
            || load_from(&root, &AppearanceFields::default()),
            |icon| crate::local_icon::apply_native_icon_path(&app, icon.map(Path::new)),
        )
        .map(|state| state.warnings)
    })
    .await
    .map_err(|error| error.to_string())?
}
#[tauri::command(async)]
pub fn save_personalization(
    app: AppHandle,
    request: PersonalizationDraft,
) -> Result<PersonalizationState, String> {
    let root = crate::storage_root::local_assets_root(&app)?;
    load_and_apply_native_icon(
        &APPLIED_NATIVE_ICON,
        || save_to(&root, &request),
        |icon| crate::local_icon::apply_native_icon_path(&app, icon.map(Path::new)),
    )
}
#[tauri::command(async)]
pub fn preview_theme_package(app: AppHandle, path: String) -> Result<ImportedTheme, String> {
    let (pack, assets) = crate::theme_zip::read_theme_package(Path::new(&path))?;
    let (stage_id, folder) = stage(&crate::storage_root::local_assets_root(&app)?, &assets)?;
    Ok(ImportedTheme {
        stage_id,
        theme: from_pack(pack, &folder, Uuid::new_v4().to_string()),
    })
}
#[tauri::command(async)]
pub fn preview_theme_sound_package(app: AppHandle, path: String) -> Result<ImportedSounds, String> {
    let (pack, assets) = crate::theme_zip::read_sound_package(Path::new(&path))?;
    let selected = pack
        .sounds
        .files
        .values()
        .map(|name| (name.clone(), assets[name].clone()))
        .collect();
    let (stage_id, folder) = stage(&crate::storage_root::local_assets_root(&app)?, &selected)?;
    Ok(ImportedSounds {
        stage_id,
        sounds: SoundSaveRequest {
            enabled: pack.sounds.enabled,
            volume: pack.sounds.volume,
            source_paths: pack
                .sounds
                .files
                .into_iter()
                .map(|(event, path)| {
                    (
                        event,
                        Some(folder.join(path).to_string_lossy().into_owned()),
                    )
                })
                .collect(),
        },
    })
}
#[tauri::command(async)]
pub fn export_theme_package(
    path: String,
    theme: ThemeDraft,
    overwrite: bool,
) -> Result<String, String> {
    crate::theme_zip::write_zip_assets(Path::new(&path), &build_theme_assets(&theme)?, overwrite)?;
    Ok(path)
}
#[tauri::command(async)]
pub fn discard_theme_imports(app: AppHandle, stage_ids: Vec<String>) -> Result<(), String> {
    discard_stages(&crate::storage_root::local_assets_root(&app)?, &stage_ids)
}
#[tauri::command(async)]
pub fn preview_theme_thumbnail(path: String) -> Result<String, String> {
    let image = crate::wallpaper::decode_image_file(Path::new(&path))?.thumbnail(240, 160);
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(output.into_inner())
    ))
}
#[cfg(test)]
#[path = "theme_library_tests.rs"]
mod tests;
