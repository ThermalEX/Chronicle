use crate::wallpaper;
use std::path::Path;
use tauri::{AppHandle, Manager, image::Image};

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

#[cfg(test)]
#[path = "local_icon_tests.rs"]
mod tests;
