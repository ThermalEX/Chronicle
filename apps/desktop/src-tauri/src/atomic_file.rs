use serde::Serialize;
use std::{fs, io::Write, path::Path};

/// Replaces `path` with `bytes` through a flushed temporary file in the same folder,
/// so readers see either the previous file or the complete new one.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("无效状态路径")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(".chronicle-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })()
    .map_err(|error| error.to_string());
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Writes human-readable JSON with [`write_atomic`].
pub(crate) fn write_json_pretty(path: &Path, value: &impl Serialize) -> Result<(), String> {
    write_atomic(
        path,
        &serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?,
    )
}
