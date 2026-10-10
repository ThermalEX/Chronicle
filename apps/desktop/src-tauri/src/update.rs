use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const RELEASE_PATH: [&str; 4] = ["ThermalEX", "Chronicle", "releases", "download"];
const RELEASE_FEED_URL: &str = "https://github.com/ThermalEX/Chronicle/releases.atom";

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

#[cfg(test)]
fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProgress<'a> {
    request_id: &'a str,
    phase: &'a str,
    bytes: u64,
    total_bytes: Option<u64>,
}

async fn download_bytes(
    client: &reqwest::Client,
    url: &str,
    mut report: impl FnMut(u64, Option<u64>),
) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("下载安装包失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("下载安装包失败：{error}"))?;
    let total = response.content_length();
    let mut bytes = Vec::new();
    report(0, total);
    let mut last_report = std::time::Instant::now();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("读取安装包失败：{error}"))?
    {
        bytes.extend_from_slice(&chunk);
        if last_report.elapsed() >= std::time::Duration::from_millis(100) {
            report(bytes.len() as u64, total);
            last_report = std::time::Instant::now();
        }
    }
    report(bytes.len() as u64, total);
    report(bytes.len() as u64, Some(bytes.len() as u64));
    Ok(bytes)
}

fn sha256_with_progress(bytes: &[u8], mut report: impl FnMut(u64, u64)) -> String {
    let mut digest = Sha256::new();
    let mut done = 0;
    report(0, bytes.len() as u64);
    for chunk in bytes.chunks(64 * 1024) {
        digest.update(chunk);
        done += chunk.len() as u64;
        report(done, bytes.len() as u64);
    }
    format!("{:x}", digest.finalize())
}

fn installer_path(download_directory: &Path) -> PathBuf {
    download_directory.join(format!("Chronicle-update-{}.exe", Uuid::new_v4()))
}

#[cfg(any(windows, test))]
fn installer_launch_result(code: Option<i32>) -> Result<(), String> {
    match code {
        Some(0) => Ok(()),
        Some(1223) => Err("update-install-cancelled".into()),
        _ => Err("无法启动安装包，请在下载目录中手动运行安装包".into()),
    }
}

#[cfg(windows)]
fn launch_installer(destination: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let system_root =
        std::env::var_os("SystemRoot").ok_or_else(|| "无法定位 Windows 系统目录".to_owned())?;
    let status = Command::new(
        PathBuf::from(system_root).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
    )
    .args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        include_str!("launch_installer.ps1"),
    ])
    .env("CHRONICLE_UPDATE_INSTALLER", destination)
    .creation_flags(0x08000000) // Hide the helper, not the installer or UAC prompt.
    .status()
    .map_err(|error| format!("无法启动安装包：{error}"))?;
    installer_launch_result(status.code())
}

#[tauri::command]
pub async fn fetch_release_feed() -> Result<String, String> {
    reqwest::Client::builder()
        .user_agent("Chronicle update checker")
        .build()
        .map_err(|error| format!("更新检查失败：{error}"))?
        .get(RELEASE_FEED_URL)
        .header(reqwest::header::ACCEPT, "application/atom+xml")
        .send()
        .await
        .map_err(|error| format!("更新检查失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("更新检查失败：{error}"))?
        .text()
        .await
        .map_err(|error| format!("更新检查失败：{error}"))
}

#[tauri::command]
pub async fn download_and_install_update(
    app: AppHandle,
    url: String,
    file_name: String,
    sha256_sums_url: Option<String>,
    request_id: String,
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
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|error| error.to_string())?;
    let report = |phase, bytes, total_bytes| {
        let _ = app.emit(
            "update-progress",
            UpdateProgress {
                request_id: &request_id,
                phase,
                bytes,
                total_bytes,
            },
        );
    };
    let installer = download_bytes(&client, &url, |bytes, total| {
        report("download", bytes, total)
    })
    .await?;

    if let Some(checksum_url) = sha256_sums_url {
        report("checksum", 0, None);
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
        // Hash on a worker so UI/event processing remains responsive during verification.
        let handle = app.clone();
        let verification_id = request_id.clone();
        let (installer, digest) = tauri::async_runtime::spawn_blocking(move || {
            let mut last_report = std::time::Instant::now();
            let digest = sha256_with_progress(&installer, |bytes, total| {
                if bytes == 0
                    || bytes == total
                    || last_report.elapsed() >= std::time::Duration::from_millis(100)
                {
                    let _ = handle.emit(
                        "update-progress",
                        UpdateProgress {
                            request_id: &verification_id,
                            phase: "verify",
                            bytes,
                            total_bytes: Some(total),
                        },
                    );
                    last_report = std::time::Instant::now();
                }
            });
            (installer, digest)
        })
        .await
        .map_err(|error| format!("安装包校验失败：{error}"))?;
        if !expected.eq_ignore_ascii_case(&digest) {
            return Err("安装包 SHA-256 校验失败，已取消安装".to_owned());
        }
        return save_and_launch(&app, installer, &request_id).await;
    }

    save_and_launch(&app, installer, &request_id).await
}

async fn save_and_launch(
    app: &AppHandle,
    installer: Vec<u8>,
    request_id: &str,
) -> Result<(), String> {
    let download_directory = app
        .path()
        .download_dir()
        .map_err(|error| format!("无法定位下载目录：{error}"))?;
    fs::create_dir_all(&download_directory)
        .map_err(|error| format!("无法创建下载目录：{error}"))?;
    let destination = installer_path(&download_directory);
    let temporary = destination.with_extension("download");
    fs::write(&temporary, &installer).map_err(|error| format!("无法保存安装包：{error}"))?;
    fs::rename(&temporary, &destination).map_err(|error| format!("无法保存安装包：{error}"))?;
    let _ = app.emit(
        "update-progress",
        UpdateProgress {
            request_id,
            phase: "launch",
            bytes: 1,
            total_bytes: Some(1),
        },
    );

    #[cfg(target_os = "windows")]
    {
        tauri::async_runtime::spawn_blocking(move || launch_installer(&destination))
            .await
            .map_err(|error| format!("无法启动安装包：{error}"))??;
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
    fn verification_progress_tracks_bytes_and_reaches_total() {
        let bytes = vec![b'a'; 200_000];
        let mut progress = Vec::new();
        let digest =
            super::sha256_with_progress(&bytes, |done, total| progress.push((done, total)));
        assert_eq!(digest, super::sha256_hex(&bytes));
        assert_eq!(progress.first(), Some(&(0, 200_000)));
        assert_eq!(progress.last(), Some(&(200_000, 200_000)));
        assert!(progress.len() > 2);
        assert!(progress.windows(2).all(|p| p[0].0 < p[1].0));
    }
    #[tokio::test]
    async fn download_reports_bytes_even_without_content_length() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = server.accept().unwrap();
            let mut request = [0; 4096];
            stream.read(&mut request).unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nabc")
                .unwrap();
        });
        let mut updates = Vec::new();
        let bytes = super::download_bytes(
            &reqwest::Client::new(),
            &format!("http://{address}"),
            |done, total| updates.push((done, total)),
        )
        .await
        .unwrap();
        worker.join().unwrap();
        assert_eq!(bytes, b"abc");
        assert_eq!(updates.first(), Some(&(0, None)));
        assert_eq!(updates.last(), Some(&(3, Some(3))));
        assert!(updates.iter().any(|p| p == &(3, None)));
    }

    #[test]
    fn installer_launch_only_succeeds_after_authorization() {
        assert!(super::installer_launch_result(Some(0)).is_ok());
        assert_eq!(
            super::installer_launch_result(Some(1223)),
            Err("update-install-cancelled".into())
        );
        assert!(super::installer_launch_result(Some(1)).is_err());
        assert!(super::installer_launch_result(None).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn elevation_helper_handles_success_cancel_and_failure() {
        use std::os::windows::process::CommandExt;
        // Replace only the interactive OS boundary; execute the shipped helper unchanged.
        for (response, expected) in [
            (
                "return [System.Diagnostics.Process]::GetCurrentProcess()",
                0,
            ),
            (
                "throw [System.ComponentModel.Win32Exception]::new(1223)",
                1223,
            ),
            ("throw [System.ComponentModel.Win32Exception]::new(2)", 1),
            ("return $null", 1),
        ] {
            let script = format!(
                "function Start-Process {{ param($FilePath, $Verb, $WindowStyle, [switch]$PassThru, $ErrorAction) if ($FilePath -cne $env:CHRONICLE_UPDATE_INSTALLER -or $Verb -ne 'RunAs' -or !$PassThru) {{ throw 'Invalid launch' }} {response} }}\n{}",
                include_str!("launch_installer.ps1")
            );
            let status = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                .env(
                    "CHRONICLE_UPDATE_INSTALLER",
                    "C:\\中文 空格\\it's $(not-code)\\setup.exe",
                )
                .creation_flags(0x08000000)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(expected));
        }
    }

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
