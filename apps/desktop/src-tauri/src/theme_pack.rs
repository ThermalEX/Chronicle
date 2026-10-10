use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceFields {
    pub color_theme: String,
    pub custom_accent: Option<String>,
    pub color_mode: String,
}
impl Default for AppearanceFields {
    fn default() -> Self {
        Self {
            color_theme: "teal".into(),
            custom_accent: None,
            color_mode: "system".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundSaveRequest {
    pub enabled: bool,
    pub volume: u8,
    #[serde(default)]
    pub source_paths: BTreeMap<String, Option<String>>,
}
impl Default for SoundSaveRequest {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 60,
            source_paths: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeDraft {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub appearance: AppearanceFields,
    pub transparency: u8,
    pub blur_px: u8,
    pub icon_source_path: Option<String>,
    pub wallpaper_source_paths: Vec<String>,
    pub selected_wallpaper_index: usize,
    pub wallpaper_playback: String,
    pub interval_seconds: u32,
    pub sounds: SoundSaveRequest,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalizationDraft {
    pub format_version: u8,
    pub mode: String,
    pub solid: AppearanceFields,
    pub selected_theme_id: Option<String>,
    pub themes: Vec<ThemeDraft>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ThemeSoundPack {
    pub enabled: bool,
    pub volume: u8,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}
impl Default for ThemeSoundPack {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 60,
            files: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePackV2 {
    pub format_version: u8,
    #[serde(skip_serializing)]
    pub name: String,
    #[serde(flatten)]
    pub appearance: AppearanceFields,
    pub transparency: u8,
    pub blur_px: u8,
    pub icon: Option<String>,
    pub wallpapers: Vec<String>,
    pub selected_wallpaper_index: usize,
    pub wallpaper_playback: String,
    pub interval_seconds: u32,
    #[serde(default)]
    pub sounds: ThemeSoundPack,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundPackV1 {
    pub format_version: u8,
    pub name: Option<String>,
    pub sounds: ThemeSoundPack,
}

pub fn validate_appearance(value: &AppearanceFields) -> Result<(), String> {
    if ![
        "teal", "indigo", "violet", "amber", "rose", "gray", "custom",
    ]
    .contains(&value.color_theme.as_str())
        || !["light", "dark", "system"].contains(&value.color_mode.as_str())
    {
        return Err("主题配置无效".into());
    }
    if value.color_theme == "custom" || value.custom_accent.is_some() {
        let accent = value.custom_accent.as_deref().unwrap_or("");
        if accent.len() != 7
            || !accent.starts_with('#')
            || !accent.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err("主题强调色无效".into());
        }
    }
    Ok(())
}
fn validate_name(name: &str) -> Result<(), String> {
    if !(1..=64).contains(&name.trim().chars().count()) {
        return Err("主题名称须为 1–64 个字符".into());
    }
    Ok(())
}
pub fn validate_sound_pack(sounds: &ThemeSoundPack) -> Result<(), String> {
    if sounds.volume > 100
        || sounds
            .files
            .keys()
            .any(|key| !crate::local_sound::EVENTS.contains(&key.as_str()))
    {
        return Err("主题音效配置无效".into());
    }
    Ok(())
}
fn validate_wallpapers(
    count: usize,
    index: usize,
    mode: &str,
    interval: u32,
    transparency: u8,
    blur: u8,
) -> Result<(), String> {
    if count > 32
        || (count == 0 && index != 0)
        || (count > 0 && index >= count)
        || !["fixed", "intervalRandom", "startupRandom"].contains(&mode)
        || !(1..=86400).contains(&interval)
        || transparency > 45
        || blur > 24
    {
        return Err("主题背景配置无效".into());
    }
    Ok(())
}
pub fn parse_theme_config(bytes: &[u8], fallback_name: &str) -> Result<ThemePackV2, String> {
    if bytes.len() > 65536 {
        return Err("主题配置文件过大".into());
    }
    let mut value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    match value
        .get("formatVersion")
        .and_then(serde_json::Value::as_u64)
    {
        Some(1) => {
            let object = value.as_object_mut().ok_or("主题配置无效")?;
            object.insert("formatVersion".into(), 2.into());
            object.entry("name").or_insert(fallback_name.into());
            object.entry("colorTheme").or_insert("custom".into());
            let wallpaper = object.remove("wallpaper").filter(|v| !v.is_null());
            object.insert(
                "wallpapers".into(),
                serde_json::Value::Array(wallpaper.into_iter().collect()),
            );
            object.insert("selectedWallpaperIndex".into(), 0.into());
            object.insert("wallpaperPlayback".into(), "fixed".into());
            object.insert("intervalSeconds".into(), 60.into());
        }
        Some(2) => {}
        _ => return Err("不支持的主题格式版本".into()),
    }
    if value.get("sounds").is_some_and(serde_json::Value::is_null) {
        value.as_object_mut().unwrap().remove("sounds");
    }
    // Package identity comes from its filename, including legacy packages with a name field.
    value
        .as_object_mut()
        .ok_or("主题配置无效")?
        .insert("name".into(), fallback_name.trim().into());
    let mut pack: ThemePackV2 = serde_json::from_value(value).map_err(|e| e.to_string())?;
    pack.name = pack.name.trim().into();
    validate_name(&pack.name)?;
    validate_appearance(&pack.appearance)?;
    validate_wallpapers(
        pack.wallpapers.len(),
        pack.selected_wallpaper_index,
        &pack.wallpaper_playback,
        pack.interval_seconds,
        pack.transparency,
        pack.blur_px,
    )?;
    validate_sound_pack(&pack.sounds)?;
    Ok(pack)
}
pub fn validate_draft(draft: &PersonalizationDraft) -> Result<(), String> {
    if draft.format_version != 1 || !["solid", "custom"].contains(&draft.mode.as_str()) {
        return Err("个性化配置无效".into());
    }
    validate_appearance(&draft.solid)?;
    let mut ids = std::collections::BTreeSet::new();
    for theme in &draft.themes {
        let id = uuid::Uuid::parse_str(&theme.id).map_err(|_| "主题标识无效")?;
        if !ids.insert(id) {
            return Err("主题标识重复".into());
        }
        validate_name(&theme.name)?;
        validate_appearance(&theme.appearance)?;
        validate_wallpapers(
            theme.wallpaper_source_paths.len(),
            theme.selected_wallpaper_index,
            &theme.wallpaper_playback,
            theme.interval_seconds,
            theme.transparency,
            theme.blur_px,
        )?;
        if theme.sounds.volume > 100
            || theme
                .sounds
                .source_paths
                .keys()
                .any(|key| !crate::local_sound::EVENTS.contains(&key.as_str()))
        {
            return Err("主题音效配置无效".into());
        }
    }
    if draft
        .selected_theme_id
        .as_ref()
        .is_some_and(|id| !draft.themes.iter().any(|theme| &theme.id == id))
        || (draft.mode == "custom" && draft.selected_theme_id.is_none())
    {
        return Err("请选择一个主题".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "theme_pack_tests.rs"]
mod tests;
