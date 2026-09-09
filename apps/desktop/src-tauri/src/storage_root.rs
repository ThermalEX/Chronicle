use std::{
    io,
    path::{Path, PathBuf},
};

pub(crate) fn resolve_repository_root(
    executable: &Path,
    app_local_data: &Path,
) -> io::Result<PathBuf> {
    let executable_directory = executable
        .parent()
        .ok_or_else(|| io::Error::other("executable has no parent directory"))?;

    if executable_directory.join("portable.marker").is_file() {
        Ok(executable_directory.join("Chronicle-data"))
    } else {
        Ok(app_local_data.join("Chronicle"))
    }
}
