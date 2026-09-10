use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const RELEASE_PATH: [&str; 4] = ["ThermalEX", "Chronicle", "releases", "download"];

fn is_windows_installer_file_name(file_name: &str) -> bool {
    file_name.starts_with("Chronicle_")
        && file_name.ends_with("_x64-setup.exe")
        && file_name.len() > "Chronicle__x64-setup.exe".len()
        && !file_name.contains(['/', '\\'])
}

fn is_trusted_release_asset(url: &str, expected_file_name: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        return false;
    }
    let Some(segments) = url.path_segments() else {
        return false;
    };
    let segments: Vec<_> = segments.collect();
    segments.len() == 6
        && segments[..4] == RELEASE_PATH
        && !segments[4].is_empty()
        && segments[5] == expected_file_name
}

fn is_trusted_installer(url: &str, file_name: &str) -> bool {
    is_windows_installer_file_name(file_name) && is_trusted_release_asset(url, file_name)
}

fn is_trusted_checksum(url: &str) -> bool {
    is_trusted_release_asset(url, "SHA256SUMS.txt")
}

fn checksum_for_file<'a>(contents: &'a str, file_name: &str) -> Option<&'a str> {
    contents.lines().find_map(|line| {
        let mut parts = line.split_ascii_whitespace();
        let checksum = parts.next()?;
        let listed_name = parts.next()?.trim_start_matches('*');
        (parts.next().is_none() && listed_name == file_name).then_some(checksum)
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn installer_path(download_directory: &Path) -> PathBuf {
    download_directory.join(format!("Chronicle-update-{}.exe", Uuid::new_v4()))
}

#[tauri::command]
pub async fn download_and_install_update(
    app: AppHandle,
    url: String,
    file_name: String,
    sha256_sums_url: Option<String>,
) -> Result<(), String> {
    if !is_trusted_installer(&url, &file_name) {
        return Err("更新安装包来源无效，已取消下载".to_owned());
    }
    if let Some(checksum_url) = &sha256_sums_url {
        if !is_trusted_checksum(checksum_url) {
            return Err("更新校验文件来源无效，已取消下载".to_owned());
        }
    }

    let client = reqwest::Client::builder()
        .user_agent("Chronicle update checker")
        .build()
        .map_err(|error| error.to_string())?;
    let installer = client
        .get(&url)
        .send()
        .await
        .map_err(|error| format!("下载安装包失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("下载安装包失败：{error}"))?
        .bytes()
        .await
        .map_err(|error| format!("读取安装包失败：{error}"))?;

    if let Some(checksum_url) = sha256_sums_url {
        let checksums = client
            .get(checksum_url)
            .send()
            .await
            .map_err(|error| format!("下载更新校验文件失败：{error}"))?
            .error_for_status()
            .map_err(|error| format!("下载更新校验文件失败：{error}"))?
            .text()
            .await
            .map_err(|error| format!("读取更新校验文件失败：{error}"))?;
        let expected = checksum_for_file(&checksums, &file_name)
            .ok_or_else(|| "更新校验文件中缺少安装包的 SHA-256".to_owned())?;
        if !expected.eq_ignore_ascii_case(&sha256_hex(&installer)) {
            return Err("安装包 SHA-256 校验失败，已取消安装".to_owned());
        }
    }

    let download_directory = app
        .path()
        .download_dir()
        .map_err(|error| format!("无法定位下载目录：{error}"))?;
    fs::create_dir_all(&download_directory).map_err(|error| format!("无法创建下载目录：{error}"))?;
    let destination = installer_path(&download_directory);
    let temporary = destination.with_extension("download");
    fs::write(&temporary, &installer).map_err(|error| format!("无法保存安装包：{error}"))?;
    fs::rename(&temporary, &destination).map_err(|error| format!("无法保存安装包：{error}"))?;

    #[cfg(target_os = "windows")]
    {
        Command::new(&destination)
            .spawn()
            .map_err(|error| format!("无法启动安装包：{error}"))?;
        app.exit(0);
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = destination;
        Err("直接安装仅支持 Windows".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{checksum_for_file, is_trusted_installer};

    #[test]
    fn accepts_only_this_projects_windows_installer_assets() {
        assert!(is_trusted_installer(
            "https://github.com/ThermalEX/Chronicle/releases/download/v1.2.0/Chronicle_1.2.0_x64-setup.exe",
            "Chronicle_1.2.0_x64-setup.exe",
        ));
        assert!(!is_trusted_installer(
            "https://example.com/Chronicle_1.2.0_x64-setup.exe",
            "Chronicle_1.2.0_x64-setup.exe",
        ));
        assert!(!is_trusted_installer(
            "https://github.com/ThermalEX/Chronicle/releases/download/v1.2.0/Chronicle_1.2.0_portable.exe",
            "Chronicle_1.2.0_portable.exe",
        ));
    }

    #[test]
    fn finds_the_exact_installer_checksum() {
        let sums = "abcdef  Chronicle_1.2.0_x64-setup.exe\n123456  other.exe\n";

        assert_eq!(
            checksum_for_file(sums, "Chronicle_1.2.0_x64-setup.exe"),
            Some("abcdef")
        );
        assert_eq!(checksum_for_file(sums, "missing.exe"), None);
    }
}
