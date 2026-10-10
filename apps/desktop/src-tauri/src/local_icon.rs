use crate::local_sound::{self, SoundAsset, SoundSaveRequest};
use crate::wallpaper;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    path::{Component, Path, PathBuf},
};
use tauri::{AppHandle, Manager, image::Image};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconSaveRequest {
    mode: String,
    source_path: Option<String>,
    remove_image: bool,
}

fn save_icon_to(root: &Path, request: IconSaveRequest) -> Result<wallpaper::WallpaperView, String> {
    // Keep device-only assets out of the synchronized repository; reuse the image
    // validation and recoverable preference swap already used for wallpapers.
    wallpaper::save_to(
        &root.join("app-icon"),
        wallpaper::WallpaperSaveRequest {
            mode: request.mode,
            source_path: request.source_path,
            remove_image: request.remove_image,
            transparency: 0,
            blur_px: 0,
        },
    )
}

pub(crate) fn apply_native_icon(app: &AppHandle, view: &mut wallpaper::WallpaperView) {
    let custom = view
        .image_data_url
        .as_deref()
        .filter(|_| view.mode == "image");
    let image = custom
        .and_then(|url| {
            let encoded = url.split_once(',')?.1;
            let bytes = STANDARD.decode(encoded).ok()?;
            let decoded = image::ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .ok()?
                .decode()
                .ok()?;
            Some(native_icon_image(decoded))
        })
        .or_else(|| {
            app.default_window_icon()
                .map(|icon| Image::new_owned(icon.rgba().to_vec(), icon.width(), icon.height()))
        });
    let Some(image) = image else {
        view.warning = Some("native-icon-unavailable".into());
        return;
    };
    if set_native_icon(app, image).is_err() {
        view.warning = Some("native-icon-unavailable".into());
    }
}

fn native_icon_image(decoded: image::DynamicImage) -> Image<'static> {
    let resized = decoded.thumbnail(128, 128).to_rgba8();
    let mut square = image::RgbaImage::new(128, 128);
    image::imageops::overlay(
        &mut square,
        &resized,
        i64::from((128 - resized.width()) / 2),
        i64::from((128 - resized.height()) / 2),
    );
    Image::new_owned(square.into_raw(), 128, 128)
}

fn native_icon_image_from_path(path: &Path) -> Result<Image<'static>, String> {
    wallpaper::decode_image_file(path).map(native_icon_image)
}

pub(crate) fn apply_native_icon_path(app: &AppHandle, path: Option<&Path>) -> Result<(), String> {
    let image = if let Some(path) = path {
        native_icon_image_from_path(path).map_err(|_| "native-icon-unavailable".to_owned())?
    } else {
        app.default_window_icon()
            .cloned()
            .ok_or_else(|| "native-icon-unavailable".to_owned())?
    };
    set_native_icon(app, image)
}

fn set_native_icon(app: &AppHandle, image: Image<'_>) -> Result<(), String> {
    let mut failed = false;
    if let Some(window) = app.get_webview_window("main") {
        failed |= window.set_icon(image.clone()).is_err();
    }
    if let Some(tray) = app.tray_by_id("main-tray") {
        failed |= tray.set_icon(Some(image)).is_err();
    }
    if failed {
        Err("native-icon-unavailable".into())
    } else {
        Ok(())
    }
}

#[tauri::command(async)]
pub fn load_local_icon(app: AppHandle) -> Result<wallpaper::WallpaperView, String> {
    let root = crate::storage_root::local_assets_root(&app)?;
    let mut view = wallpaper::load_from(&root.join("app-icon"))?;
    apply_native_icon(&app, &mut view);
    Ok(view)
}

#[tauri::command(async)]
pub fn save_local_icon(
    app: AppHandle,
    request: IconSaveRequest,
) -> Result<wallpaper::WallpaperView, String> {
    let root = crate::storage_root::local_assets_root(&app)?;
    let mut view = save_icon_to(&root, request)?;
    apply_native_icon(&app, &mut view);
    Ok(view)
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThemePack {
    format_version: u8,
    #[serde(default = "custom_theme")]
    color_theme: String,
    custom_accent: String,
    color_mode: String,
    transparency: u8,
    blur_px: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wallpaper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sounds: Option<ThemeSoundPack>,
}

#[derive(Deserialize, Serialize)]
struct ThemeSoundPack {
    enabled: bool,
    volume: u8,
    files: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThemeSoundView {
    enabled: bool,
    volume: u8,
    files: BTreeMap<String, SoundAsset>,
    source_paths: BTreeMap<String, String>,
}

fn custom_theme() -> String {
    "custom".into()
}

impl ThemePack {
    fn validate(&self) -> Result<(), String> {
        if self.sounds.as_ref().is_some_and(|sounds| {
            sounds.volume > 100
                || sounds
                    .files
                    .keys()
                    .any(|key| !local_sound::EVENTS.contains(&key.as_str()))
        }) {
            return Err("主题音效配置无效".into());
        }
        if self.format_version != 1
            || !matches!(
                self.color_theme.as_str(),
                "teal" | "indigo" | "violet" | "amber" | "rose" | "gray" | "custom"
            )
            || !matches!(self.color_mode.as_str(), "light" | "dark" | "system")
            || self.transparency > 45
            || self.blur_px > 24
            || self.custom_accent.len() != 7
            || !self.custom_accent.starts_with('#')
            || !self.custom_accent[1..]
                .bytes()
                .all(|c| c.is_ascii_hexdigit())
        {
            return Err("主题配置无效".into());
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeExportRequest {
    color_theme: String,
    custom_accent: String,
    color_mode: String,
    transparency: u8,
    blur_px: u8,
    icon_mode: String,
    icon_source_path: Option<String>,
    wallpaper_mode: String,
    wallpaper_source_path: Option<String>,
    sounds: Option<SoundSaveRequest>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePackView {
    color_theme: String,
    custom_accent: String,
    color_mode: String,
    transparency: u8,
    blur_px: u8,
    icon_path: String,
    wallpaper_path: String,
    icon_data_url: String,
    wallpaper_data_url: String,
    sounds: Option<ThemeSoundView>,
}

fn pack_asset(folder: &Path, name: &str) -> Result<PathBuf, String> {
    let relative = Path::new(name);
    if name.is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("主题资源必须位于主题包文件夹内".into());
    }
    let folder = folder.canonicalize().map_err(|e| e.to_string())?;
    let path = folder
        .join(relative)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(&folder) {
        return Err("主题资源必须位于主题包文件夹内".into());
    }
    Ok(path)
}

fn preview_pack(path: &Path) -> Result<ThemePackView, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 64 * 1024 {
        return Err("主题配置文件过大".into());
    }
    let pack: ThemePack = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("主题配置无效：{e}"))?;
    pack.validate()?;
    let folder = path.parent().ok_or("主题配置缺少父目录")?;
    let icon = pack
        .icon
        .as_deref()
        .map(|name| pack_asset(folder, name))
        .transpose()?;
    let wallpaper = pack
        .wallpaper
        .as_deref()
        .map(|name| pack_asset(folder, name))
        .transpose()?;
    let sounds = pack
        .sounds
        .map(|sounds| -> Result<ThemeSoundView, String> {
            let mut files = BTreeMap::new();
            let mut source_paths = BTreeMap::new();
            for (event, name) in sounds.files {
                let path = pack_asset(folder, &name)?;
                files.insert(event.clone(), local_sound::preview_sound(&path)?);
                source_paths.insert(event, path.to_string_lossy().into_owned());
            }
            Ok(ThemeSoundView {
                enabled: sounds.enabled,
                volume: sounds.volume,
                files,
                source_paths,
            })
        })
        .transpose()?;
    Ok(ThemePackView {
        color_theme: pack.color_theme,
        custom_accent: pack.custom_accent.to_ascii_lowercase(),
        color_mode: pack.color_mode,
        transparency: pack.transparency,
        blur_px: pack.blur_px,
        sounds,
        icon_data_url: icon
            .as_deref()
            .map(wallpaper::preview_from_path)
            .transpose()?
            .unwrap_or_default(),
        wallpaper_data_url: wallpaper
            .as_deref()
            .map(wallpaper::preview_from_path)
            .transpose()?
            .unwrap_or_default(),
        icon_path: icon
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        wallpaper_path: wallpaper
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
    })
}

fn export_pack(
    root: &Path,
    directory: &Path,
    request: ThemeExportRequest,
) -> Result<PathBuf, String> {
    let mut pack = ThemePack {
        format_version: 1,
        color_theme: request.color_theme,
        custom_accent: request.custom_accent.to_ascii_lowercase(),
        color_mode: request.color_mode,
        transparency: request.transparency,
        blur_px: request.blur_px,
        icon: None,
        wallpaper: None,
        sounds: None,
    };
    pack.validate()?;
    if !matches!(request.icon_mode.as_str(), "color" | "image")
        || !matches!(request.wallpaper_mode.as_str(), "color" | "image")
    {
        return Err("主题配置无效".into());
    }
    // Validate and copy into memory first. Export must not save local preferences.
    let icon = if request.icon_mode == "image" {
        Some(wallpaper::export_image(
            &root.join("app-icon"),
            request.icon_source_path.as_deref(),
        )?)
    } else {
        None
    };
    let wallpaper = if request.wallpaper_mode == "image" {
        Some(wallpaper::export_image(
            root,
            request.wallpaper_source_path.as_deref(),
        )?)
    } else {
        None
    };
    pack.icon = icon.as_ref().map(|(_, ext)| format!("icon.{ext}"));
    pack.wallpaper = wallpaper
        .as_ref()
        .map(|(_, ext)| format!("background.{ext}"));
    let sounds = request
        .sounds
        .as_ref()
        .map(|request| local_sound::resolved_sound_bytes(root, request))
        .transpose()?
        .unwrap_or_default();
    pack.sounds = request.sounds.map(|request| ThemeSoundPack {
        enabled: request.enabled,
        volume: request.volume,
        files: sounds
            .keys()
            .map(|key| (key.clone(), format!("sounds/{key}.wav")))
            .collect(),
    });
    let json = serde_json::to_vec_pretty(&pack).map_err(|e| e.to_string())?;
    let directory = directory.canonicalize().map_err(|e| e.to_string())?;
    if !directory.is_dir() {
        return Err("请选择主题导出目录".into());
    }
    let folder = directory.join(format!("Chronicle-theme-{}", Uuid::new_v4()));
    fs::create_dir(&folder).map_err(|e| e.to_string())?;
    let manifest = folder.join("theme.json");
    let write = || -> Result<(), std::io::Error> {
        if !sounds.is_empty() {
            fs::create_dir(folder.join("sounds"))?;
            for (event, bytes) in &sounds {
                fs::write(folder.join(format!("sounds/{event}.wav")), bytes)?;
            }
        }
        if let (Some((bytes, _)), Some(name)) = (&icon, &pack.icon) {
            fs::write(folder.join(name), bytes)?;
        }
        if let (Some((bytes, _)), Some(name)) = (&wallpaper, &pack.wallpaper) {
            fs::write(folder.join(name), bytes)?;
        }
        // Publish the manifest last, so incomplete output is not a usable pack.
        fs::write(&manifest, json)
    };
    if let Err(error) = write() {
        for event in sounds.keys() {
            let _ = fs::remove_file(folder.join(format!("sounds/{event}.wav")));
        }
        if !sounds.is_empty() {
            let _ = fs::remove_dir(folder.join("sounds"));
        }
        // Only remove files owned by this export, never previous exports.
        for name in [
            Some("theme.json"),
            pack.icon.as_deref(),
            pack.wallpaper.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let _ = fs::remove_file(folder.join(name));
        }
        let _ = fs::remove_dir(&folder);
        return Err(error.to_string());
    }
    Ok(manifest)
}

#[tauri::command(async)]
pub fn export_local_theme(
    app: AppHandle,
    directory: String,
    request: ThemeExportRequest,
) -> Result<String, String> {
    let root = crate::storage_root::local_assets_root(&app)?;
    export_pack(&root, Path::new(&directory), request)
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command(async)]
pub fn preview_local_theme(path: String) -> Result<ThemePackView, String> {
    preview_pack(Path::new(&path))
}

#[cfg(test)]
#[path = "local_icon_tests.rs"]
mod tests;
