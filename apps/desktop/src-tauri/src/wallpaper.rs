use std::{
    fs::{self, File},
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const MAX_IMAGE_BYTES: u64 = 15 * 1024 * 1024;
const FOLDER: &str = "wallpaper";
const PREFERENCES: &str = "preferences.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallpaperSaveRequest {
    mode: String,
    transparency: u8,
    blur_px: u8,
    source_path: Option<String>,
    remove_image: bool,
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
    mode: String,
    transparency: u8,
    blur_px: u8,
    image_data_url: Option<String>,
    warning: Option<String>,
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
    if !path.exists() {
        return Ok(WallpaperPreferences::default());
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn image_bytes(path: &Path) -> Result<(Vec<u8>, &'static str, &'static str), String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return Err("图片必须是小于或等于 15 MiB 的文件".into());
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
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
    reader.decode().map_err(|error| error.to_string())?;
    Ok((bytes, mime, ext))
}

fn data_url(bytes: &[u8], mime: &str) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

fn preview_from_path(path: &Path) -> Result<String, String> {
    let (bytes, mime, _) = image_bytes(path)?;
    Ok(data_url(&bytes, mime))
}

fn load_from(root: &Path) -> Result<WallpaperView, String> {
    let (preferences, mut warning) = match read_preferences(root) {
        Ok(value) => (value, None),
        Err(error) => (
            WallpaperPreferences::default(),
            Some(format!("本机壁纸设置已损坏：{error}")),
        ),
    };
    let image_data_url = match preferences.image_filename.as_deref() {
        Some(name) if valid_filename(name) => {
            match preview_from_path(&wallpaper_dir(root).join(name)) {
                Ok(url) => Some(url),
                Err(error) => {
                    warning = Some(format!("本机壁纸无法读取：{error}"));
                    None
                }
            }
        }
        Some(_) => {
            warning = Some("本机壁纸文件名无效".into());
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

fn write_preferences(path: &Path, preferences: &WallpaperPreferences) -> Result<(), String> {
    let folder = path.parent().ok_or("设置路径缺少父目录")?;
    let temporary = folder.join(format!(".{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec(preferences).map_err(|error| error.to_string())?;
    let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    drop(file);
    let backup = folder.join(format!(".{}.backup", Uuid::new_v4()));
    if path.exists() {
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

fn save_to(root: &Path, request: WallpaperSaveRequest) -> Result<WallpaperView, String> {
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

fn local_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn load_local_wallpaper(app: AppHandle) -> Result<WallpaperView, String> {
    load_from(&local_root(&app)?)
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
    save_to(&local_root(&app)?, request)
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
