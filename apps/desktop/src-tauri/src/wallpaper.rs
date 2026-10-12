use std::{
    fs::{self, File},
    io::{Cursor, Write},
    path::Path,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageFormat, ImageReader};
use serde::Serialize;
use tauri::AppHandle;
use uuid::Uuid;

const MAX_IMAGE_BYTES: u64 = 15 * 1024 * 1024;
const PREFERENCES_BACKUP: &str = "preferences.json.backup";

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

pub(crate) fn export_image(path: &Path) -> Result<(Vec<u8>, &'static str), String> {
    let (bytes, _, ext) = image_bytes(path)?;
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::preview_from_path;

    #[test]
    fn fake_oversized_or_unsupported_images_are_rejected() {
        let root = tempdir().unwrap();
        let fake = root.path().join("fake.jpg");
        fs::write(&fake, b"not a JPEG").unwrap();
        assert!(preview_from_path(&fake).is_err());
        let oversized = root.path().join("large.png");
        fs::write(&oversized, vec![0; 15 * 1024 * 1024 + 1]).unwrap();
        assert!(preview_from_path(&oversized).is_err());
        let unsupported = root.path().join("animated.gif");
        fs::write(&unsupported, b"GIF89a").unwrap();
        assert!(preview_from_path(&unsupported).is_err());
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
