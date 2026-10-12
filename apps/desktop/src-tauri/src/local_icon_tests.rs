use super::*;
use std::fs;
use tempfile::tempdir;
#[test]
fn native_path_icon_is_centered_and_rejects_invalid_images() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("wide.png");
    image::RgbaImage::from_pixel(40, 20, image::Rgba([200, 30, 40, 255]))
        .save(&path)
        .unwrap();
    let icon = native_icon_image_from_path(&path).unwrap();
    assert_eq!((icon.width(), icon.height()), (128, 128));
    assert_eq!(&icon.rgba()[0..4], &[0, 0, 0, 0]);
    let center = (64 * 128 + 64) * 4;
    assert_eq!(&icon.rgba()[center..center + 4], &[200, 30, 40, 255]);
    fs::write(&path, b"invalid").unwrap();
    assert!(native_icon_image_from_path(&path).is_err());
}
