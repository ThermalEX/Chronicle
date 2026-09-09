use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::storage_root::resolve_repository_root;

fn test_directory(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("chronicle-{name}-{}", uuid::Uuid::new_v4()))
}

#[test]
fn portable_marker_uses_sibling_data_directory() {
    let directory = test_directory("portable-root");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("portable.marker"), "").unwrap();

    let result =
        resolve_repository_root(&directory.join("Chronicle.exe"), Path::new("C:/AppData")).unwrap();

    assert_eq!(result, directory.join("Chronicle-data"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn normal_installation_uses_app_local_data() {
    let directory = test_directory("installed-root");

    let result =
        resolve_repository_root(&directory.join("Chronicle.exe"), Path::new("C:/AppData")).unwrap();

    assert_eq!(result, PathBuf::from("C:/AppData").join("Chronicle"));
}
