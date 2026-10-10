use std::{
    fs::{self, File},
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use uuid::Uuid;

const MAX_IMAGE_BYTES: u64 = 15 * 1024 * 1024;
const FOLDER: &str = "wallpaper";
const PREFERENCES: &str = "preferences.json";
const PREFERENCES_BACKUP: &str = "preferences.json.backup";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallpaperSaveRequest {
    pub(crate) mode: String,
    pub(crate) transparency: u8,
    pub(crate) blur_px: u8,
    pub(crate) source_path: Option<String>,
    pub(crate) remove_image: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct WallpaperPreferences {
    mode: String,
    transparency: u8,
    blur_px: u8,
    image_filename: Option<String>,
}

impl Default for WallpaperPreferences {
    fn default() -> Self {
        Self {
            mode: "color".into(),
            transparency: 28,
            blur_px: 12,
            image_filename: None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallpaperView {
    pub(crate) mode: String,
    pub(crate) transparency: u8,
    pub(crate) blur_px: u8,
    pub(crate) image_data_url: Option<String>,
    pub(crate) warning: Option<String>,
}

fn wallpaper_dir(root: &Path) -> PathBuf {
    root.join(FOLDER)
}

fn valid_filename(name: &str) -> bool {
    name.starts_with("wallpaper-")
        && !name.contains(['/', '\\'])
        && matches!(
            Path::new(name).extension().and_then(|ext| ext.to_str()),
            Some("png" | "jpg" | "webp")
        )
}

fn read_preferences(root: &Path) -> Result<WallpaperPreferences, String> {
    let path = wallpaper_dir(root).join(PREFERENCES);
    let backup = wallpaper_dir(root).join(PREFERENCES_BACKUP);
    let readable = if path.exists() { &path } else { &backup };
    if !readable.exists() {
        return Ok(WallpaperPreferences::default());
    }
    let bytes = fs::read(readable).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn image_bytes(path: &Path) -> Result<(Vec<u8>, &'static str, &'static str), String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return Err("图片必须是小于或等于 15 MiB 的文件".into());
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let (mime, ext) = validate_image_bytes(&bytes)?;
    Ok((bytes, mime, ext))
}

pub(crate) fn validate_image_bytes(bytes: &[u8]) -> Result<(&'static str, &'static str), String> {
    decode_image_bytes(bytes).map(|(_, mime, ext)| (mime, ext))
}

fn decode_image_bytes(
    bytes: &[u8],
) -> Result<(image::DynamicImage, &'static str, &'static str), String> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("图片必须是小于或等于 15 MiB 的文件".into());
    }
    let reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let (mime, ext) = match reader.format() {
        Some(ImageFormat::Png) => ("image/png", "png"),
        Some(ImageFormat::Jpeg) => ("image/jpeg", "jpg"),
        Some(ImageFormat::WebP) => ("image/webp", "webp"),
        _ => return Err("仅支持 PNG、JPEG 和 WebP 图片".into()),
    };
    if reader.format() == Some(ImageFormat::WebP)
        && image::codecs::webp::WebPDecoder::new(Cursor::new(&bytes))
            .map_err(|error| error.to_string())?
            .has_animation()
    {
        return Err("不支持动画 WebP 图片".into());
    }
    let image = reader.decode().map_err(|error| error.to_string())?;
    Ok((image, mime, ext))
}

pub(crate) fn decode_image_file(path: &Path) -> Result<image::DynamicImage, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return Err("图片必须是小于或等于 15 MiB 的文件".into());
    }
    decode_image_bytes(&fs::read(path).map_err(|error| error.to_string())?)
        .map(|(image, _, _)| image)
}

fn data_url(bytes: &[u8], mime: &str) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

pub(crate) fn export_image(
    root: &Path,
    source: Option<&str>,
) -> Result<(Vec<u8>, &'static str), String> {
    let path = if let Some(source) = source {
        PathBuf::from(source)
    } else {
        let name = read_preferences(root)?
            .image_filename
            .filter(|name| valid_filename(name))
            .ok_or("主题图片无法读取，请重新选择图片")?;
        wallpaper_dir(root).join(name)
    };
    let (bytes, _, ext) = image_bytes(&path)?;
    Ok((bytes, ext))
}

pub(crate) fn preview_from_path(path: &Path) -> Result<String, String> {
    let (bytes, mime, _) = image_bytes(path)?;
    Ok(data_url(&bytes, mime))
}

pub(crate) fn read_wallpaper_image(root: &Path, path: &Path) -> Result<Vec<u8>, String> {
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return Err("图片必须是小于或等于 15 MiB 的文件".into());
    }
    // Registered assets were validated when copied. The browser decodes them at display time.
    // External draft images still undergo the complete import validation, never a wildcard scope.
    let saved = root.join("personalization/preferences.json").is_file()
        && crate::theme_library::load_from(root, &crate::theme_pack::AppearanceFields::default())
            .is_ok_and(|state| {
                state.draft.themes.iter().any(|theme| {
                    theme.wallpaper_source_paths.iter().any(|source| {
                        Path::new(source)
                            .canonicalize()
                            .is_ok_and(|source| source == path)
                    })
                })
            });
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    if saved {
        if !matches!(
            image::guess_format(&bytes),
            Ok(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)
        ) {
            return Err("仅支持 PNG、JPEG 和 WebP 图片".into());
        }
    } else {
        validate_image_bytes(&bytes)?;
    }
    Ok(bytes)
}

#[tauri::command(async)]
pub fn load_wallpaper_image(app: AppHandle, path: String) -> Result<tauri::ipc::Response, String> {
    read_wallpaper_image(
        &crate::storage_root::local_assets_root(&app)?,
        Path::new(&path),
    )
    .map(tauri::ipc::Response::new)
}

pub(crate) fn load_from(root: &Path) -> Result<WallpaperView, String> {
    let (preferences, mut warning) = match read_preferences(root) {
        Ok(value) => (value, None),
        Err(_) => (
            WallpaperPreferences::default(),
            Some("preferences-damaged".into()),
        ),
    };
    let image_data_url = match preferences.image_filename.as_deref() {
        Some(name) if valid_filename(name) => {
            match preview_from_path(&wallpaper_dir(root).join(name)) {
                Ok(url) => Some(url),
                Err(_) => {
                    warning = Some("image-unreadable".into());
                    None
                }
            }
        }
        Some(_) => {
            warning = Some("image-invalid".into());
            None
        }
        None => None,
    };
    let mode = if preferences.mode == "image" && image_data_url.is_some() {
        "image"
    } else {
        "color"
    };
    Ok(WallpaperView {
        mode: mode.into(),
        transparency: preferences.transparency.min(45),
        blur_px: preferences.blur_px.min(24),
        image_data_url,
        warning,
    })
}

pub(crate) fn write_preferences<T: Serialize>(path: &Path, preferences: &T) -> Result<(), String> {
    let folder = path.parent().ok_or("设置路径缺少父目录")?;
    let temporary = folder.join(format!(".{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec(preferences).map_err(|error| error.to_string())?;
    let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    drop(file);
    let backup = folder.join(PREFERENCES_BACKUP);
    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| error.to_string())?;
        }
        fs::rename(path, &backup).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(error.to_string());
    }
    if backup.exists() {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

pub(crate) fn save_to(root: &Path, request: WallpaperSaveRequest) -> Result<WallpaperView, String> {
    if !matches!(request.mode.as_str(), "color" | "image")
        || request.transparency > 45
        || request.blur_px > 24
    {
        return Err("壁纸设置无效".into());
    }
    if request.remove_image && request.source_path.is_some() {
        return Err("不能同时移除和选择图片".into());
    }
    let folder = wallpaper_dir(root);
    fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let previous = read_preferences(root).unwrap_or_default();
    let mut image_filename = if request.remove_image {
        None
    } else {
        previous.image_filename.clone()
    };
    let mut staged_image = None;
    if let Some(source_path) = request.source_path.as_deref() {
        let (bytes, _, ext) = image_bytes(Path::new(source_path))?;
        let filename = format!("wallpaper-{}.{}", Uuid::new_v4(), ext);
        let target = folder.join(&filename);
        let mut file = File::create(&target).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        image_filename = Some(filename);
        staged_image = Some(target);
    }
    if request.mode == "image"
        && image_filename
            .as_deref()
            .filter(|name| valid_filename(name) && folder.join(name).is_file())
            .is_none()
    {
        if let Some(path) = staged_image {
            let _ = fs::remove_file(path);
        }
        return Err("请先选择有效的壁纸图片".into());
    }
    let preferences = WallpaperPreferences {
        mode: request.mode,
        transparency: request.transparency,
        blur_px: request.blur_px,
        image_filename: image_filename.clone(),
    };
    if let Err(error) = write_preferences(&folder.join(PREFERENCES), &preferences) {
        if let Some(path) = staged_image {
            let _ = fs::remove_file(path);
        }
        return Err(error);
    }
    if let Some(old) = previous
        .image_filename
        .filter(|old| valid_filename(old) && Some(old) != image_filename.as_ref())
    {
        let _ = fs::remove_file(folder.join(old));
    }
    load_from(root)
}

#[tauri::command(async)]
pub fn load_local_wallpaper(app: AppHandle) -> Result<WallpaperView, String> {
    load_from(&crate::storage_root::local_assets_root(&app)?)
}

#[tauri::command(async)]
pub fn preview_local_wallpaper(path: String) -> Result<String, String> {
    preview_from_path(Path::new(&path))
}

#[tauri::command(async)]
pub fn save_local_wallpaper(
    app: AppHandle,
    request: WallpaperSaveRequest,
) -> Result<WallpaperView, String> {
    save_to(&crate::storage_root::local_assets_root(&app)?, request)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{WallpaperSaveRequest, load_from, preview_from_path, save_to};

    const PNG: &[u8] = include_bytes!("../../src/assets/icon.png");

    impl WallpaperSaveRequest {
        fn with_source(path: &std::path::Path) -> Self {
            Self {
                mode: "image".into(),
                transparency: 28,
                blur_px: 12,
                source_path: Some(path.to_string_lossy().into_owned()),
                remove_image: false,
            }
        }
    }

    #[test]
    fn wallpaper_defaults_to_local_color_mode() {
        let root = tempdir().unwrap();
        let view = load_from(root.path()).unwrap();
        assert_eq!(view.mode, "color");
        assert_eq!(view.transparency, 28);
        assert_eq!(view.blur_px, 12);
        assert!(view.image_data_url.is_none());
    }

    #[test]
    fn copied_wallpaper_survives_original_removal() {
        let root = tempdir().unwrap();
        let source = root.path().join("选中的图片.png");
        fs::write(&source, PNG).unwrap();
        let saved = save_to(
            root.path(),
            WallpaperSaveRequest {
                mode: "image".into(),
                transparency: 28,
                blur_px: 12,
                source_path: Some(source.to_string_lossy().into_owned()),
                remove_image: false,
            },
        )
        .unwrap();
        assert!(
            saved
                .image_data_url
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        fs::remove_file(source).unwrap();
        assert_eq!(load_from(root.path()).unwrap().mode, "image");
    }

    #[test]
    fn invalid_replacement_keeps_previous_wallpaper() {
        let root = tempdir().unwrap();
        let good = root.path().join("good.png");
        fs::write(&good, PNG).unwrap();
        save_to(root.path(), WallpaperSaveRequest::with_source(&good)).unwrap();
        let invalid = root.path().join("fake.jpg");
        fs::write(&invalid, b"not a JPEG").unwrap();
        assert!(preview_from_path(&invalid).is_err());
        assert!(save_to(root.path(), WallpaperSaveRequest::with_source(&invalid)).is_err());
        assert_eq!(load_from(root.path()).unwrap().mode, "image");
    }

    #[test]
    fn damaged_preferences_fall_back_without_changing_the_library() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("wallpaper")).unwrap();
        fs::write(root.path().join("wallpaper/preferences.json"), b"{").unwrap();
        let view = load_from(root.path()).unwrap();
        assert_eq!(view.mode, "color");
        assert!(view.warning.is_some());
    }

    #[test]
    fn interrupted_preferences_swap_reads_previous_image_and_settings() {
        let root = tempdir().unwrap();
        let source = root.path().join("good.png");
        fs::write(&source, PNG).unwrap();
        save_to(root.path(), WallpaperSaveRequest::with_source(&source)).unwrap();
        let folder = root.path().join("wallpaper");
        fs::rename(
            folder.join("preferences.json"),
            folder.join("preferences.json.backup"),
        )
        .unwrap();
        let view = load_from(root.path()).unwrap();
        assert_eq!(view.mode, "image");
        assert!(view.image_data_url.is_some());
        assert!(view.warning.is_none());
        save_to(
            root.path(),
            WallpaperSaveRequest {
                mode: "color".into(),
                transparency: 12,
                blur_px: 4,
                source_path: None,
                remove_image: false,
            },
        )
        .unwrap();
        assert!(folder.join("preferences.json").exists());
        assert!(!folder.join("preferences.json.backup").exists());
    }

    #[test]
    fn missing_saved_image_falls_back_to_color() {
        let root = tempdir().unwrap();
        let source = root.path().join("wallpaper.png");
        fs::write(&source, PNG).unwrap();
        save_to(root.path(), WallpaperSaveRequest::with_source(&source)).unwrap();
        let folder = root.path().join("wallpaper");
        let image = fs::read_dir(&folder)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.extension().and_then(|value| value.to_str()) == Some("png"))
            .unwrap();
        fs::remove_file(image).unwrap();
        let view = load_from(root.path()).unwrap();
        assert_eq!(view.mode, "color");
        assert!(view.warning.is_some());
    }

    #[test]
    fn oversized_or_unsupported_image_does_not_replace_saved_image() {
        let root = tempdir().unwrap();
        let good = root.path().join("good.png");
        fs::write(&good, PNG).unwrap();
        save_to(root.path(), WallpaperSaveRequest::with_source(&good)).unwrap();
        let oversized = root.path().join("large.png");
        fs::write(&oversized, vec![0; 15 * 1024 * 1024 + 1]).unwrap();
        assert!(save_to(root.path(), WallpaperSaveRequest::with_source(&oversized)).is_err());
        let unsupported = root.path().join("animated.gif");
        fs::write(&unsupported, b"GIF89a").unwrap();
        assert!(save_to(root.path(), WallpaperSaveRequest::with_source(&unsupported)).is_err());
        assert_eq!(load_from(root.path()).unwrap().mode, "image");
    }

    #[test]
    fn switching_to_color_keeps_the_saved_image_for_later() {
        let root = tempdir().unwrap();
        let source = root.path().join("good.png");
        fs::write(&source, PNG).unwrap();
        save_to(root.path(), WallpaperSaveRequest::with_source(&source)).unwrap();
        save_to(
            root.path(),
            WallpaperSaveRequest {
                mode: "color".into(),
                transparency: 14,
                blur_px: 4,
                source_path: None,
                remove_image: false,
            },
        )
        .unwrap();
        let view = load_from(root.path()).unwrap();
        assert_eq!(view.mode, "color");
        assert!(view.image_data_url.is_some());
        save_to(
            root.path(),
            WallpaperSaveRequest {
                mode: "image".into(),
                transparency: 14,
                blur_px: 4,
                source_path: None,
                remove_image: false,
            },
        )
        .unwrap();
        assert_eq!(load_from(root.path()).unwrap().mode, "image");
    }

    #[test]
    fn png_jpeg_and_webp_are_accepted_by_content() {
        use image::{DynamicImage, ImageFormat};
        let root = tempdir().unwrap();
        let image = DynamicImage::new_rgb8(1, 1);
        for (format, mime) in [
            (ImageFormat::Png, "image/png"),
            (ImageFormat::Jpeg, "image/jpeg"),
            (ImageFormat::WebP, "image/webp"),
        ] {
            let mut bytes = std::io::Cursor::new(Vec::new());
            image.write_to(&mut bytes, format).unwrap();
            let source = root.path().join("unknown.bin");
            fs::write(&source, bytes.into_inner()).unwrap();
            assert!(
                preview_from_path(&source)
                    .unwrap()
                    .starts_with(&format!("data:{mime};base64,"))
            );
        }
    }
}
