use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use std::{fs, path::Path};

pub(crate) const EVENTS: [&str; 5] = [
    "connected",
    "disconnected",
    "connectionFailed",
    "notification",
    "default",
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundAsset {
    pub(crate) name: String,
    pub(crate) data_url: String,
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

#[tauri::command(async)]
pub fn preview_local_sound(path: String) -> Result<SoundAsset, String> {
    preview_sound(Path::new(&path))
}

#[cfg(test)]
#[path = "local_sound_tests.rs"]
pub(crate) mod tests;
