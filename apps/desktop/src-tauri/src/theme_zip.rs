use crate::theme_pack::{SoundPackV1, ThemePackV2, parse_theme_config, validate_sound_pack};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};
const MAX_BYTES: u64 = 256 * 1024 * 1024;
type Assets = BTreeMap<String, Vec<u8>>;

pub(crate) fn validate_asset_path(name: &str) -> Result<(), String> {
    if name.is_empty() || name.starts_with('/') || name.contains(['\\', ':', '\0']) {
        return Err("主题资源路径无效".into());
    }
    for part in name.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || ['<', '>', '"', '|', '?', '*'].contains(&c))
            || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err("主题资源路径无效".into());
        }
    }
    Ok(())
}
// ZipArchive indexes by raw name and collapses exact duplicates. Inspect the
// central names first so none of those records can evade validation.
fn validate_central_names(file: &mut File, start: u64, expected: usize) -> Result<(), String> {
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut names = BTreeSet::new();
    let mut count = 0;
    loop {
        let mut header = [0; 46];
        file.read_exact(&mut header[..4])
            .map_err(|e| e.to_string())?;
        if &header[..4] != b"PK\x01\x02" {
            break;
        }
        file.read_exact(&mut header[4..])
            .map_err(|e| e.to_string())?;
        let length = u16::from_le_bytes(header[28..30].try_into().unwrap()) as usize;
        let extra = u16::from_le_bytes(header[30..32].try_into().unwrap()) as u64;
        let comment = u16::from_le_bytes(header[32..34].try_into().unwrap()) as u64;
        let mut name = vec![0; length];
        file.read_exact(&mut name).map_err(|e| e.to_string())?;
        let name = std::str::from_utf8(&name)
            .map_err(|_| "主题资源路径无效")?
            .trim_end_matches('/');
        validate_asset_path(name)?;
        if !names.insert(name.to_lowercase()) {
            return Err("主题包包含重复资源路径".into());
        }
        file.seek(SeekFrom::Current((extra + comment) as i64))
            .map_err(|e| e.to_string())?;
        count += 1;
    }
    if count != expected {
        return Err("主题包包含重复资源路径".into());
    }
    Ok(())
}
pub fn read_zip_assets(path: &Path) -> Result<Assets, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("主题包超过 256 MiB".into());
    }
    let mut archive =
        ZipArchive::new(file.try_clone().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    validate_central_names(&mut file, archive.central_directory_start(), archive.len())?;
    let mut assets = Assets::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        if entry.encrypted()
            || entry.is_symlink()
            || !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err("不支持加密、链接或特殊压缩的主题包".into());
        }
        if entry.is_dir() {
            continue;
        }
        if entry.size() > MAX_BYTES - total {
            return Err("主题包超过 256 MiB".into());
        }
        let name = entry.name().to_string();
        let mut bytes = Vec::new();
        let mut chunk = [0; 65536];
        loop {
            let count = entry.read(&mut chunk).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            total += count as u64;
            if total > MAX_BYTES {
                return Err("主题包超过 256 MiB".into());
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        assets.insert(name, bytes);
    }
    Ok(assets)
}
pub fn write_zip_assets(path: &Path, assets: &Assets, overwrite: bool) -> Result<(), String> {
    if path.exists() && !overwrite {
        return Err("导出文件已存在，请确认覆盖或选择其他名称".into());
    }
    let mut names = BTreeSet::new();
    let mut total = 0_u64;
    for (name, bytes) in assets {
        validate_asset_path(name)?;
        if !names.insert(name.to_lowercase()) {
            return Err("主题包包含重复资源路径".into());
        }
        total += bytes.len() as u64;
        if total > MAX_BYTES {
            return Err("主题包超过 256 MiB".into());
        }
    }
    let parent = path.parent().ok_or("导出路径无效")?;
    let temporary = parent.join(format!(".theme-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        let mut writer = ZipWriter::new(file);
        for (name, bytes) in assets {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )
                .map_err(|e| e.to_string())?;
            writer.write_all(bytes).map_err(|e| e.to_string())?;
        }
        let file = writer.finish().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES {
            return Err("主题包超过 256 MiB".into());
        }
        drop(file);
        if overwrite {
            fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        } else {
            fs::hard_link(&temporary, path).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}
fn validate_references(pack: &ThemePackV2, assets: &Assets) -> Result<(), String> {
    for name in pack.wallpapers.iter().chain(pack.icon.iter()) {
        validate_asset_path(name)?;
        crate::wallpaper::validate_image_bytes(assets.get(name).ok_or("主题资源缺失")?)?;
    }
    validate_sounds(&pack.sounds, assets)
}
fn validate_sounds(
    sounds: &crate::theme_pack::ThemeSoundPack,
    assets: &Assets,
) -> Result<(), String> {
    validate_sound_pack(sounds)?;
    for name in sounds.files.values() {
        validate_asset_path(name)?;
        crate::local_sound::validate_sound_bytes(assets.get(name).ok_or("主题音效资源缺失")?)?;
    }
    Ok(())
}
pub fn read_theme_package(path: &Path) -> Result<(ThemePackV2, Assets), String> {
    let fallback = path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("导入主题");
    let (pack, assets) = if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        if fs::metadata(path).map_err(|e| e.to_string())?.len() > 65536 {
            return Err("主题配置文件过大".into());
        }
        let folder = path
            .parent()
            .ok_or("主题路径无效")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let fallback = folder
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or(fallback);
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let pack = parse_theme_config(&bytes, fallback)?;
        let mut assets = Assets::from([("theme.json".into(), bytes)]);
        let mut total = 0_u64;
        for name in pack
            .wallpapers
            .iter()
            .chain(pack.icon.iter())
            .chain(pack.sounds.files.values())
        {
            validate_asset_path(name)?;
            let mut target = folder.clone();
            for part in name.split('/') {
                target.push(part);
                if fs::symlink_metadata(&target)
                    .map_err(|e| e.to_string())?
                    .file_type()
                    .is_symlink()
                {
                    return Err("主题资源不支持链接".into());
                }
            }
            let target = target.canonicalize().map_err(|e| e.to_string())?;
            if !target.starts_with(&folder) {
                return Err("主题资源路径无效".into());
            }
            let metadata = fs::metadata(&target).map_err(|e| e.to_string())?;
            if !metadata.is_file() || metadata.len() > 15 * 1024 * 1024 {
                return Err("图片必须是小于或等于 15 MiB 的文件".into());
            }
            total += metadata.len();
            if total > MAX_BYTES {
                return Err("主题包超过 256 MiB".into());
            }
            assets.insert(name.clone(), fs::read(target).map_err(|e| e.to_string())?);
        }
        (pack, assets)
    } else {
        let assets = read_zip_assets(path)?;
        let pack = parse_theme_config(
            assets.get("theme.json").ok_or("主题包缺少 theme.json")?,
            fallback,
        )?;
        (pack, assets)
    };
    validate_references(&pack, &assets)?;
    Ok((pack, assets))
}
pub fn read_sound_package(path: &Path) -> Result<(SoundPackV1, Assets), String> {
    let assets = read_zip_assets(path)?;
    let pack = if let Some(bytes) = assets.get("sounds.json") {
        if bytes.len() > 65536 {
            return Err("主题配置文件过大".into());
        }
        let pack: SoundPackV1 = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if pack.format_version != 1 {
            return Err("不支持的声音包格式版本".into());
        }
        pack
    } else {
        let theme = parse_theme_config(
            assets.get("theme.json").ok_or("声音包缺少 sounds.json")?,
            "导入主题",
        )?;
        SoundPackV1 {
            format_version: 1,
            name: Some(theme.name),
            sounds: theme.sounds,
        }
    };
    validate_sounds(&pack.sounds, &assets)?;
    Ok((pack, assets))
}
#[cfg(test)]
#[path = "theme_zip_tests.rs"]
pub(crate) mod tests;
