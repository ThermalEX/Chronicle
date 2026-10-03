use std::{fs, path::Path};

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DroppedPathDto {
    pub path: String,
    pub name: String,
    pub kind: Option<&'static str>,
    pub exists: bool,
    pub error: Option<String>,
}

fn describe_paths(paths: Vec<String>) -> Vec<DroppedPathDto> {
    paths
        .into_iter()
        .map(|path| {
            let candidate = Path::new(&path);
            let name = candidate
                .file_name()
                .map(|part| part.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            let result = if path.contains('\0') {
                Err("路径包含无效字符".to_owned())
            } else {
                fs::metadata(candidate)
                    .map_err(|error| error.to_string())
                    .and_then(|metadata| {
                        if metadata.is_file() {
                            Ok("file")
                        } else if metadata.is_dir() {
                            Ok("folder")
                        } else {
                            Err("仅支持文件和文件夹".to_owned())
                        }
                    })
            };
            match result {
                Ok(kind) => DroppedPathDto {
                    path,
                    name,
                    kind: Some(kind),
                    exists: true,
                    error: None,
                },
                Err(error) => DroppedPathDto {
                    path,
                    name,
                    kind: None,
                    exists: false,
                    error: Some(error),
                },
            }
        })
        .collect()
}

#[tauri::command(async)]
pub fn describe_dropped_paths(paths: Vec<String>) -> Vec<DroppedPathDto> {
    describe_paths(paths)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::describe_paths;

    #[test]
    fn describes_file_folder_unicode_and_missing_paths_independently() {
        let root = tempdir().unwrap();
        let file = root.path().join("存档.txt");
        let folder = root.path().join("游戏目录");
        fs::write(&file, b"save").unwrap();
        fs::create_dir(&folder).unwrap();
        let paths = vec![
            file.to_string_lossy().into_owned(),
            folder.to_string_lossy().into_owned(),
            root.path().join("missing").to_string_lossy().into_owned(),
            "bad\0path".to_owned(),
        ];
        let results = describe_paths(paths);
        assert_eq!(results.len(), 4);
        assert_eq!(results[0].name, "存档.txt");
        assert_eq!(results[0].kind.as_deref(), Some("file"));
        assert!(results[0].exists);
        assert_eq!(results[1].name, "游戏目录");
        assert_eq!(results[1].kind.as_deref(), Some("folder"));
        assert!(results[1].exists);
        assert!(!results[2].exists);
        assert!(results[2].error.is_some());
        assert!(!results[3].exists);
        assert!(results[3].error.is_some());
    }
}
