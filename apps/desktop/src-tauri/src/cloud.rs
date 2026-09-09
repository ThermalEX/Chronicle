#![allow(clippy::needless_pass_by_value)]

use std::{
    fs,
    io::{BufReader, Read, Write},
    path::{Component, Path, PathBuf},
};

use chronicle_core::SyncMode;
use chronicle_sync::{
    GitHubChange, GitHubClient, GitHubError, GitHubSource, OpenDalSource, RemoteStore,
    RequestPolicy, WebDavClient, WebDavError, WebDavSource,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::AppState;

const CREDENTIAL_SERVICE: &str = "Chronicle WebDAV";
const GITHUB_CREDENTIAL_SERVICE: &str = "Chronicle GitHub";
const OPENDAL_CREDENTIAL_SERVICE: &str = "Chronicle OpenDAL";

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloudSourceInput {
    id: String,
    name: String,
    provider: String,
    endpoint: String,
    username: String,
    remote_path: String,
    credential_ref: String,
    #[serde(default)]
    repository: String,
    #[serde(default = "default_branch")]
    branch: String,
    #[serde(default)]
    scheme: String,
    #[serde(default)]
    config: std::collections::HashMap<String, String>,
    #[serde(default)]
    secret_keys: Vec<String>,
    #[serde(default)]
    sync_enabled: bool,
}

fn default_branch() -> String {
    "main".into()
}

#[derive(Clone, Debug, Deserialize)]
struct Catalog {
    #[serde(default)]
    entries: Vec<CatalogEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CatalogEntry {
    id: String,
    name: String,
    folder: String,
    #[serde(default)]
    category_id: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    storage_policy: Value,
    #[serde(default)]
    sync_mode: Value,
    #[serde(default)]
    source_count: usize,
    #[serde(default)]
    snapshot_count: usize,
    #[serde(default)]
    stored_bytes: u64,
    #[serde(default)]
    last_snapshot_at_ms: Option<u64>,
    #[serde(default)]
    sources: Vec<Value>,
    #[serde(default)]
    created_at_ms: u64,
    #[serde(default)]
    snapshots: Vec<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteItemDto {
    id: String,
    name: String,
    kind: String,
    protected: bool,
    snapshot_count: usize,
    size_bytes: u64,
    updated_at: Option<u64>,
    sync_mode: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudPreviewDto {
    source_name: String,
    library_id: Option<String>,
    items: Vec<RemoteItemDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResultDto {
    status: String,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedGitHubRepositoryDto {
    repository: String,
    branch: String,
}

/// Deliberately contains no credential value; the renderer only needs to know
/// whether an existing source has a credential in Windows Credential Manager.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSourceStatusDto {
    source_id: String,
    credential_saved: bool,
}

fn storage_error() -> String {
    "Chronicle 本地仓库状态不可用".into()
}

fn policy(settings: &Value) -> RequestPolicy {
    let cloud = settings.get("cloud").unwrap_or(&Value::Null);
    RequestPolicy {
        max_concurrent_metadata_reads: usize::try_from(
            cloud
                .get("maxConcurrentMetadataReads")
                .and_then(Value::as_u64)
                .unwrap_or(2),
        )
        .unwrap_or(2),
        max_concurrent_transfers: usize::try_from(
            cloud
                .get("maxConcurrentTransfers")
                .and_then(Value::as_u64)
                .unwrap_or(2),
        )
        .unwrap_or(2),
        request_delay_ms: cloud
            .get("requestDelayMs")
            .and_then(Value::as_u64)
            .unwrap_or(150),
        retry_limit: cloud
            .get("retryLimit")
            .and_then(Value::as_u64)
            .unwrap_or(5)
            .min(10) as u8,
        ..RequestPolicy::default()
    }
}

fn unix_millis() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

fn source_from_settings(settings: &Value, source_id: &str) -> Result<CloudSourceInput, String> {
    settings
        .pointer("/cloud/sources")
        .and_then(Value::as_array)
        .and_then(|sources| {
            sources
                .iter()
                .find(|source| source.get("id").and_then(Value::as_str) == Some(source_id))
        })
        .cloned()
        .ok_or_else(|| "找不到云同步源".to_owned())
        .and_then(|mut source| {
            match source["provider"].as_str() {
                Some("github") => source["provider"] = Value::from("legacy_github"),
                Some("webdav") => source["provider"] = Value::from("legacy_webdav"),
                _ => {}
            }
            serde_json::from_value(source).map_err(|_| "同步源配置无效".into())
        })
}

fn raw_credential(source: &CloudSourceInput) -> Result<String, String> {
    let account = if source.credential_ref.is_empty() {
        &source.id
    } else {
        &source.credential_ref
    };
    let service = if source.provider == "legacy_github" {
        GITHUB_CREDENTIAL_SERVICE
    } else if source.provider == "opendal" {
        OPENDAL_CREDENTIAL_SERVICE
    } else {
        CREDENTIAL_SERVICE
    };
    keyring::Entry::new(service, account)
        .map_err(|error| error.to_string())?
        .get_password()
        .map_err(|_| "该同步源尚未保存密码".to_owned())
}

#[tauri::command(async)]
pub async fn cloud_source_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<CloudSourceStatusDto>, String> {
    let settings = state
        .repository
        .lock()
        .map_err(|_| storage_error())?
        .load_settings()
        .map_err(|error| error.to_string())?;
    let source_ids = settings
        .pointer("/cloud/sources")
        .and_then(Value::as_array)
        .map(|sources| {
            sources
                .iter()
                .filter_map(|source| source.get("id").and_then(Value::as_str))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    source_ids
        .iter()
        .map(|source_id| {
            let source = source_from_settings(&settings, source_id)?;
            let credential_saved = raw_credential(&source).is_ok();
            Ok(CloudSourceStatusDto {
                source_id: source.id,
                credential_saved,
            })
        })
        .collect()
}

fn github_client(
    source: &CloudSourceInput,
    token: String,
    request_policy: RequestPolicy,
) -> Result<GitHubClient, String> {
    GitHubClient::new(
        GitHubSource {
            repository: source.repository.clone(),
            branch: source.branch.clone(),
            remote_path: source.remote_path.clone(),
            token,
        },
        request_policy,
    )
    .map_err(|error| error.to_string())
}

fn client(
    source: &CloudSourceInput,
    password: String,
    request_policy: RequestPolicy,
) -> Result<RemoteStore, String> {
    if source.provider == "opendal" {
        let secrets = serde_json::from_str(&password)
            .map_err(|_| "OpenDAL 机密配置必须是键值对象".to_owned())?;
        return RemoteStore::opendal(
            OpenDalSource {
                scheme: source.scheme.clone(),
                root: source.remote_path.clone(),
                config: source.config.clone(),
                secret_keys: source.secret_keys.clone(),
            },
            secrets,
            request_policy,
        )
        .map_err(|error| error.to_string());
    }
    if source.provider != "legacy_webdav" {
        return Err("GitHub 同步接口尚未启用".into());
    }
    WebDavClient::new(
        WebDavSource {
            endpoint: source.endpoint.clone(),
            username: source.username.clone(),
            password,
            remote_path: source.remote_path.clone(),
        },
        request_policy,
    )
    .map(RemoteStore::LegacyWebDav)
    .map_err(|error| error.to_string())
}

fn checked_folder(value: &str) -> Result<&str, String> {
    chronicle_sync::validate_relative_path(value).map_err(|error| error.to_string())?;
    let mut components = Path::new(value).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(value),
        _ => Err("远端目录清单包含无效路径".into()),
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_reader(BufReader::new(
        fs::File::open(path).map_err(|error| error.to_string())?,
    ))
    .map_err(|error| error.to_string())
}

fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "目标路径无父目录".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(".chronicle-{}.tmp", Uuid::new_v4()));
    let mut file = fs::File::create(&temporary).map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut reader = BufReader::new(fs::File::open(path).map_err(|error| error.to_string())?);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn snapshot_ids_from_timeline(timeline: &Value) -> Vec<String> {
    timeline
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

fn missing_snapshot_archives(
    local_snapshots: &[Value],
    remote_snapshot_ids: &[String],
) -> Result<Vec<String>, String> {
    local_snapshots
        .iter()
        .filter(|snapshot| {
            snapshot
                .get("id")
                .and_then(Value::as_str)
                .is_none_or(|id| !remote_snapshot_ids.iter().any(|remote_id| remote_id == id))
        })
        .map(|snapshot| {
            snapshot
                .get("archive_name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| "本地时间线缺少压缩包名称".to_owned())
        })
        .collect()
}

fn save_sync_state(
    root: &Path,
    source_id: &str,
    entry_id: &str,
    snapshot_ids: &[String],
) -> Result<(), String> {
    let path = root.join("config/sync").join(format!("{source_id}.json"));
    let mut state = if path.is_file() {
        read_json::<Value>(&path)?
    } else {
        serde_json::json!({ "formatVersion": 1, "entries": {} })
    };
    state["entries"][entry_id] = serde_json::json!({
        "snapshotIds": snapshot_ids,
        "syncedAtMs": unix_millis()
    });
    write_json_atomic(&path, &state)
}

fn configured_source(
    state: &State<'_, AppState>,
    source_id: &str,
) -> Result<(CloudSourceInput, PathBuf, RequestPolicy), String> {
    let repository = state.repository.lock().map_err(|_| storage_error())?;
    let settings = repository
        .load_settings()
        .map_err(|error| error.to_string())?;
    let root = repository.root().to_path_buf();
    drop(repository);
    let source = source_from_settings(&settings, source_id)?;
    Ok((source, root, policy(&settings)))
}

fn configured_client(
    state: &State<'_, AppState>,
    source_id: &str,
) -> Result<(RemoteStore, CloudSourceInput, PathBuf), String> {
    let (source, root, request_policy) = configured_source(state, source_id)?;
    let client = client(&source, credential(&source)?, request_policy)?;
    Ok((client, source, root))
}

fn configured_github_client(
    state: &State<'_, AppState>,
    source_id: &str,
) -> Result<(GitHubClient, CloudSourceInput, PathBuf), String> {
    let (source, root, request_policy) = configured_source(state, source_id)?;
    if source.provider != "legacy_github" {
        return Err("该同步源不是 GitHub 仓库".into());
    }
    let client = github_client(&source, credential(&source)?, request_policy)?;
    Ok((client, source, root))
}

#[tauri::command(async)]
pub async fn save_cloud_credential(
    source_id: String,
    credential_ref: String,
    password: String,
    provider: String,
) -> Result<(), String> {
    let account = if credential_ref.is_empty() {
        source_id.as_str()
    } else {
        credential_ref.as_str()
    };
    if provider == "opendal" {
        return Err("OpenDAL 凭据必须测试后保存".into());
    }
    if !matches!(provider.as_str(), "legacy_github" | "legacy_webdav") {
        return Err("不支持的同步源类型".into());
    }
    let service = if provider == "legacy_github" {
        GITHUB_CREDENTIAL_SERVICE
    } else if provider == "opendal" {
        OPENDAL_CREDENTIAL_SERVICE
    } else {
        CREDENTIAL_SERVICE
    };
    let entry = keyring::Entry::new(service, account).map_err(|error| error.to_string())?;
    if password.is_empty() {
        entry.delete_credential().map_err(|error| error.to_string())
    } else {
        entry
            .set_password(&password)
            .map_err(|error| error.to_string())
    }
}

#[tauri::command(async)]
pub async fn test_cloud_source(source: CloudSourceInput, password: String) -> Result<(), String> {
    if source.provider == "opendal" {
        tested_sources()
            .lock()
            .map_err(|_| storage_error())?
            .remove(&source.id);
    }
    let password = if source.provider == "opendal" {
        let patch = if password.is_empty() {
            Default::default()
        } else {
            serde_json::from_str(&password).map_err(|_| "机密配置必须是键值对象".to_owned())?
        };
        serde_json::to_string(&merge_secret_patch(&source, patch)?)
            .map_err(|_| "机密配置无效".to_owned())?
    } else if password.is_empty() {
        credential(&source)?
    } else {
        password
    };
    if source.provider == "legacy_github" {
        github_client(&source, password, RequestPolicy::default())?
            .test_access()
            .await
            .map_err(|error| error.to_string())
    } else {
        let test_client = client(&source, password.clone(), RequestPolicy::default())?;
        test_client
            .test_capabilities()
            .await
            .map_err(|error| error.to_string())?;
        if source.provider == "opendal" {
            tested_sources()
                .lock()
                .map_err(|_| storage_error())?
                .insert(
                    source.id.clone(),
                    source_fingerprint(
                        &source,
                        &serde_json::from_str(&password).map_err(|_| "机密配置无效".to_owned())?,
                    )?,
                );
        }
        Ok(())
    }
}

fn tested_sources() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    static TESTED: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, String>>,
    > = std::sync::OnceLock::new();
    TESTED.get_or_init(Default::default)
}

#[derive(Serialize, Deserialize)]
struct TestedCredential {
    secrets: std::collections::HashMap<String, String>,
    fingerprint: String,
}

fn source_fingerprint(
    source: &CloudSourceInput,
    secrets: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let mut canonical = serde_json::to_value(source).map_err(|_| "配置无效".to_owned())?;
    // Synchronization state and advanced-field order are presentation only; values and target remain bound.
    canonical.as_object_mut().expect("cloud source serializes to an object").remove("syncEnabled");
    let mut keys = source.secret_keys.clone();
    keys.sort();
    canonical["secretKeys"] = serde_json::json!(keys);
    let secrets = serde_json::to_value(secrets).map_err(|_| "机密配置无效".to_owned())?;
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&(canonical, secrets)).map_err(|_| "配置无效".to_owned())?);
    Ok(format!("{:x}", hash.finalize()))
}

fn credential(source: &CloudSourceInput) -> Result<String, String> {
    let raw = raw_credential(source)?;
    if source.provider != "opendal" {
        return Ok(raw);
    }
    let bundle: TestedCredential =
        serde_json::from_str(&raw).map_err(|_| "请重新测试并保存 OpenDAL 凭据".to_owned())?;
    verify_bundle(source, &bundle)?;
    serde_json::to_string(&bundle.secrets).map_err(|_| "机密配置无效".to_owned())
}

fn verify_bundle(source: &CloudSourceInput, bundle: &TestedCredential) -> Result<(), String> {
    if source_fingerprint(source, &bundle.secrets)? != bundle.fingerprint {
        return Err("配置已更改，请重新测试并保存同步源".into());
    }
    Ok(())
}

fn merge_secret_patch(
    source: &CloudSourceInput,
    patch: std::collections::HashMap<String, String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    if source.provider != "opendal" {
        return Err("该同步源不是 OpenDAL".into());
    }
    let mut secrets = match raw_credential(source) {
        Ok(raw) => {
            serde_json::from_str::<TestedCredential>(&raw)
                .map_err(|_| "已保存的机密配置无效，请重新创建同步源".to_owned())?
                .secrets
        }
        Err(_) => Default::default(),
    };
    if patch.keys().any(|key| !source.secret_keys.contains(key)) {
        return Err("机密字段未在同步源中声明".into());
    }
    for (key, value) in patch {
        if !value.is_empty() {
            secrets.insert(key, value);
        }
    }
    secrets.retain(|key, _| source.secret_keys.contains(key));
    Ok(secrets)
}

#[tauri::command(async)]
pub async fn save_opendal_credential(
    source: CloudSourceInput,
    secrets: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let secrets = merge_secret_patch(&source, secrets)?;
    let fingerprint = source_fingerprint(&source, &secrets)?;
    if tested_sources()
        .lock()
        .map_err(|_| storage_error())?
        .get(&source.id)
        != Some(&fingerprint)
    {
        // A previously verified, unchanged bundle remains valid across app restarts.
        let valid_saved = raw_credential(&source)
            .ok()
            .and_then(|raw| serde_json::from_str::<TestedCredential>(&raw).ok())
            .is_some_and(|saved| {
                saved.fingerprint == fingerprint && verify_bundle(&source, &saved).is_ok()
            });
        if !valid_saved {
            return Err("请先测试此配置的读写、列举和清理能力".into());
        }
    }
    let bundle = TestedCredential {
        secrets,
        fingerprint,
    };
    let account = if source.credential_ref.is_empty() {
        &source.id
    } else {
        &source.credential_ref
    };
    keyring::Entry::new(OPENDAL_CREDENTIAL_SERVICE, account)
        .map_err(|_| "无法访问系统凭据库".to_owned())?
        .set_password(&serde_json::to_string(&bundle).map_err(|_| "机密配置无效".to_owned())?)
        .map_err(|_| "无法保存系统凭据".to_owned())
}

pub fn validate_settings(settings: &Value, _previous: &Value) -> Result<(), String> {
    let sources = settings.pointer("/cloud/sources").and_then(Value::as_array);
    let enabled = settings.pointer("/cloud/enabled").and_then(Value::as_bool) == Some(true);
    let mut ids = std::collections::HashSet::new();
    for value in sources.into_iter().flatten() {
        let source: CloudSourceInput =
            serde_json::from_value(value.clone()).map_err(|_| "同步源配置无效".to_owned())?;
        if !ids.insert(source.id.clone()) || source.id.is_empty() {
            return Err("同步源标识重复或为空".into());
        }
        if !matches!(
            source.provider.as_str(),
            "legacy_github" | "legacy_webdav" | "opendal"
        ) {
            return Err("不支持的同步源类型".into());
        }
        if source.provider != "opendal" {
            continue;
        }
        if !chronicle_sync::OPEN_DAL_SCHEMES.contains(&source.scheme.as_str())
            || source.config.keys().any(|key| {
                !chronicle_sync::PUBLIC_CONFIG_KEYS.contains(&key.as_str())
                    || source.secret_keys.contains(key)
            })
        {
            return Err("不支持的服务，或公开配置包含机密字段".into());
        }
        if enabled && source.sync_enabled {
            credential(&source)?;
        }
    }
    Ok(())
}

#[tauri::command(async)]
pub async fn create_github_repository(
    source: CloudSourceInput,
    repository_name: String,
    password: String,
) -> Result<CreatedGitHubRepositoryDto, String> {
    if source.provider != "legacy_github" {
        return Err("该同步源不是 GitHub 仓库".into());
    }
    let password = if password.is_empty() {
        credential(&source)?
    } else {
        password
    };
    let repository = GitHubClient::create_private_repository(
        &password,
        &repository_name,
        RequestPolicy::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(CreatedGitHubRepositoryDto {
        repository: repository.repository,
        branch: repository.branch,
    })
}

#[tauri::command(async)]
pub async fn cloud_preview(
    state: State<'_, AppState>,
    source_id: String,
) -> Result<CloudPreviewDto, String> {
    let (source, _, _) = configured_source(&state, &source_id)?;
    if source.provider == "legacy_github" {
        let (client, source, root) = configured_github_client(&state, &source_id)?;
        return github_preview(&client, source, &root).await;
    }
    let (client, source, root) = configured_client(&state, &source_id)?;
    let library = ensure_remote_library(&client, &root).await?;
    let catalog: Option<Catalog> = match client.get_json("catalog.json").await {
        Ok(value) => Some(value),
        Err(WebDavError::NotFound(_)) => None,
        Err(error) => return Err(error.to_string()),
    };
    let mut items = vec![
        RemoteItemDto {
            id: "config:library".into(),
            name: "library.json".into(),
            kind: "config".into(),
            protected: true,
            snapshot_count: 0,
            size_bytes: 0,
            updated_at: library.get("updatedAtMs").and_then(Value::as_u64),
            sync_mode: "manual".into(),
        },
        RemoteItemDto {
            id: "config:catalog".into(),
            name: "catalog.json".into(),
            kind: "config".into(),
            protected: true,
            snapshot_count: 0,
            size_bytes: 0,
            updated_at: None,
            sync_mode: "manual".into(),
        },
    ];
    if let Some(catalog) = catalog {
        items.extend(catalog.entries.into_iter().map(|entry| RemoteItemDto {
            id: entry.id,
            name: entry.name,
            kind: "archive".into(),
            protected: false,
            snapshot_count: entry.snapshot_count,
            size_bytes: entry.stored_bytes,
            updated_at: entry.last_snapshot_at_ms,
            sync_mode: entry.sync_mode.as_str().unwrap_or("manual").to_owned(),
        }));
    }
    Ok(CloudPreviewDto {
        source_name: source.name,
        library_id: library
            .get("libraryId")
            .and_then(Value::as_str)
            .map(str::to_owned),
        items,
    })
}

async fn ensure_remote_layout(client: &RemoteStore) -> Result<(), String> {
    for path in ["", "archives"] {
        client
            .ensure_collection(path)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn updated_library(root: &Path) -> Result<Value, String> {
    let library_path = root.join("library.json");
    let mut library = if library_path.is_file() {
        read_json::<Value>(&library_path)?
    } else {
        serde_json::json!({ "formatVersion": 2, "libraryId": Uuid::new_v4().to_string() })
    };
    library["updatedAtMs"] = Value::from(unix_millis());
    write_json_atomic(&library_path, &library)?;
    Ok(library)
}

async fn ensure_remote_library(client: &RemoteStore, root: &Path) -> Result<Value, String> {
    match client.get_json("library.json").await {
        Ok(value) => Ok(value),
        Err(WebDavError::NotFound(_)) => {
            ensure_remote_layout(client).await?;
            let library_path = root.join("library.json");
            let mut library = if library_path.is_file() {
                read_json::<Value>(&library_path)?
            } else {
                serde_json::json!({ "formatVersion": 2, "libraryId": Uuid::new_v4().to_string() })
            };
            library["updatedAtMs"] = Value::from(unix_millis());
            write_json_atomic(&library_path, &library)?;
            client
                .put_json("library.json", &library)
                .await
                .map_err(|error| error.to_string())?;

            match client.get_json::<Value>("catalog.json").await {
                Ok(_) => {}
                Err(WebDavError::NotFound(_)) => {
                    client
                        .put_json(
                            "catalog.json",
                            &serde_json::json!({
                                "format_version": 4,
                                "updated_at_ms": unix_millis(),
                                "categories": [],
                                "entries": []
                            }),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                }
                Err(error) => return Err(error.to_string()),
            }
            Ok(library)
        }
        Err(error) => Err(error.to_string()),
    }
}

async fn ensure_github_library(client: &GitHubClient, root: &Path) -> Result<Value, String> {
    match client.get_json("library.json").await {
        Ok(value) => Ok(value),
        Err(GitHubError::NotFound(_)) => {
            let library_path = root.join("library.json");
            let mut library = if library_path.is_file() {
                read_json::<Value>(&library_path)?
            } else {
                serde_json::json!({ "formatVersion": 2, "libraryId": Uuid::new_v4().to_string() })
            };
            library["updatedAtMs"] = Value::from(unix_millis());
            write_json_atomic(&library_path, &library)?;
            client
                .commit_changes(
                    "Initialize Chronicle repository",
                    vec![
                        GitHubChange { path: "library.json".into(), contents: Some(serde_json::to_vec_pretty(&library).map_err(|error| error.to_string())?) },
                        GitHubChange { path: "catalog.json".into(), contents: Some(serde_json::to_vec_pretty(&serde_json::json!({ "format_version": 4, "updated_at_ms": unix_millis(), "categories": [], "entries": [] })).map_err(|error| error.to_string())?) },
                    ],
                )
                .await
                .map_err(|error| error.to_string())?;
            Ok(library)
        }
        Err(error) => Err(error.to_string()),
    }
}

async fn github_preview(
    client: &GitHubClient,
    source: CloudSourceInput,
    root: &Path,
) -> Result<CloudPreviewDto, String> {
    let library = ensure_github_library(client, root).await?;
    let catalog: Option<Catalog> = match client.get_json("catalog.json").await {
        Ok(value) => Some(value),
        Err(GitHubError::NotFound(_)) => None,
        Err(error) => return Err(error.to_string()),
    };
    let mut items = vec![
        RemoteItemDto {
            id: "config:library".into(),
            name: "library.json".into(),
            kind: "config".into(),
            protected: true,
            snapshot_count: 0,
            size_bytes: 0,
            updated_at: library.get("updatedAtMs").and_then(Value::as_u64),
            sync_mode: "manual".into(),
        },
        RemoteItemDto {
            id: "config:catalog".into(),
            name: "catalog.json".into(),
            kind: "config".into(),
            protected: true,
            snapshot_count: 0,
            size_bytes: 0,
            updated_at: None,
            sync_mode: "manual".into(),
        },
    ];
    if let Some(catalog) = catalog {
        items.extend(catalog.entries.into_iter().map(|entry| RemoteItemDto {
            id: entry.id,
            name: entry.name,
            kind: "archive".into(),
            protected: false,
            snapshot_count: entry.snapshot_count,
            size_bytes: entry.stored_bytes,
            updated_at: entry.last_snapshot_at_ms,
            sync_mode: entry.sync_mode.as_str().unwrap_or("manual").to_owned(),
        }));
    }
    Ok(CloudPreviewDto {
        source_name: source.name,
        library_id: library
            .get("libraryId")
            .and_then(Value::as_str)
            .map(str::to_owned),
        items,
    })
}

async fn github_overwrite_upload(
    client: &GitHubClient,
    root: &Path,
    source_id: &str,
    entry_id: &str,
) -> Result<(), String> {
    let local_catalog: Value = read_json(&root.join("catalog.json"))?;
    let entry: CatalogEntry = serde_json::from_value(
        local_catalog
            .get("entries")
            .and_then(Value::as_array)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.get("id").and_then(Value::as_str) == Some(entry_id))
            })
            .cloned()
            .ok_or_else(|| "本地存档不存在".to_owned())?,
    )
    .map_err(|error| error.to_string())?;
    let mut remote_catalog: Value = match client.get_json("catalog.json").await {
        Ok(value) => value,
        Err(GitHubError::NotFound(_)) => {
            serde_json::json!({ "format_version": 4, "updated_at_ms": unix_millis(), "categories": [], "entries": [] })
        }
        Err(error) => return Err(error.to_string()),
    };
    merge_catalog_entry(&mut remote_catalog, &local_catalog, entry_id)?;
    remote_catalog["updated_at_ms"] = Value::from(unix_millis());
    let folder = checked_folder(&entry.folder)?;
    let local_dir = root.join("archives").join(folder);
    let mut changes = Vec::new();
    for item in WalkDir::new(&local_dir).min_depth(1) {
        let item = item.map_err(|error| error.to_string())?;
        if !item.file_type().is_file() {
            continue;
        }
        let relative = item
            .path()
            .strip_prefix(&local_dir)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        changes.push(GitHubChange {
            path: format!("archives/{folder}/{relative}"),
            contents: Some(fs::read(item.path()).map_err(|error| error.to_string())?),
        });
    }
    let library_path = root.join("library.json");
    let mut library = if library_path.is_file() {
        read_json::<Value>(&library_path)?
    } else {
        serde_json::json!({ "formatVersion": 2, "libraryId": Uuid::new_v4().to_string() })
    };
    library["updatedAtMs"] = Value::from(unix_millis());
    write_json_atomic(&library_path, &library)?;
    changes.push(GitHubChange {
        path: "catalog.json".into(),
        contents: Some(
            serde_json::to_vec_pretty(&remote_catalog).map_err(|error| error.to_string())?,
        ),
    });
    changes.push(GitHubChange {
        path: "library.json".into(),
        contents: Some(serde_json::to_vec_pretty(&library).map_err(|error| error.to_string())?),
    });
    client
        .commit_changes(&format!("Sync Chronicle archive {}", entry.name), changes)
        .await
        .map_err(|error| error.to_string())?;
    save_sync_state(
        root,
        source_id,
        entry_id,
        &snapshot_ids_from_timeline(&Value::Array(entry.snapshots)),
    )
}

async fn github_overwrite_download(
    client: &GitHubClient,
    root: &Path,
    source_id: &str,
    entry_id: &str,
) -> Result<(), String> {
    let remote_catalog_value: Value = client
        .get_json("catalog.json")
        .await
        .map_err(|error| error.to_string())?;
    let remote_catalog: Catalog =
        serde_json::from_value(remote_catalog_value.clone()).map_err(|error| error.to_string())?;
    let remote_entry = remote_catalog
        .entries
        .iter()
        .find(|entry| entry.id == entry_id)
        .ok_or_else(|| "远端存档不存在".to_owned())?;
    let folder = checked_folder(&remote_entry.folder)?;
    let staging = root
        .join(".tmp")
        .join(format!("github-download-{}", Uuid::new_v4()));
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    for snapshot in &remote_entry.snapshots {
        let archive_name = snapshot
            .get("archive_name")
            .and_then(Value::as_str)
            .ok_or_else(|| "远端时间线缺少压缩包名称".to_owned())?;
        checked_folder(archive_name)?;
        let expected_hash = snapshot
            .get("object_hash")
            .and_then(Value::as_str)
            .ok_or_else(|| "远端时间线缺少校验值".to_owned())?;
        let target = staging.join(archive_name);
        fs::write(
            &target,
            client
                .download_file(&format!("archives/{folder}/{archive_name}"))
                .await
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if hash_file(&target)? != expected_hash {
            let _ = fs::remove_dir_all(&staging);
            return Err("远端快照校验失败，本地内容未更改".into());
        }
    }
    let target = root.join("archives").join(folder);
    let backup = root
        .join(".tmp")
        .join(format!("github-old-{}", Uuid::new_v4()));
    if target.exists() {
        fs::rename(&target, &backup).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&staging, &target) {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(error.to_string());
    }
    let local_catalog_path = root.join("catalog.json");
    let mut local_catalog: Value = read_json(&local_catalog_path)?;
    let entries = local_catalog
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "本地清单格式无效".to_owned())?;
    entries.retain(|entry| entry.get("id").and_then(Value::as_str) != Some(entry_id));
    let summary = remote_catalog_value
        .get("entries")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(entry_id))
        })
        .cloned()
        .ok_or_else(|| "远端清单缺少存档".to_owned())?;
    entries.push(summary);
    if let Err(error) = write_json_atomic(&local_catalog_path, &local_catalog) {
        let _ = fs::remove_dir_all(&target);
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(error);
    }
    if backup.exists() {
        fs::remove_dir_all(backup).map_err(|error| error.to_string())?;
    }
    save_sync_state(
        root,
        source_id,
        entry_id,
        &snapshot_ids_from_timeline(&Value::Array(remote_entry.snapshots.clone())),
    )
}

async fn github_merge_remote_snapshots(
    client: &GitHubClient,
    root: &Path,
    local_entry: &CatalogEntry,
    remote_entry: &CatalogEntry,
) -> Result<(), String> {
    let local_folder = checked_folder(&local_entry.folder)?;
    let remote_folder = checked_folder(&remote_entry.folder)?;
    let entry_dir = root.join("archives").join(local_folder);
    fs::create_dir_all(&entry_dir).map_err(|error| error.to_string())?;
    let mut local_timeline = Value::Array(local_entry.snapshots.clone());
    let local_ids = snapshot_ids_from_timeline(&local_timeline);
    let local_array = local_timeline
        .as_array_mut()
        .ok_or_else(|| "本地时间线格式无效".to_owned())?;

    for snapshot in remote_entry.snapshots.iter().filter(|snapshot| {
        !local_ids
            .iter()
            .any(|id| Some(id.as_str()) == snapshot.get("id").and_then(Value::as_str))
    }) {
        let archive_name = snapshot
            .get("archive_name")
            .and_then(Value::as_str)
            .ok_or_else(|| "远端时间线缺少压缩包名称".to_owned())?;
        checked_folder(archive_name)?;
        let expected_hash = snapshot
            .get("object_hash")
            .and_then(Value::as_str)
            .ok_or_else(|| "远端时间线缺少校验值".to_owned())?;
        let temporary = root
            .join(".tmp")
            .join(format!("github-merge-{}", Uuid::new_v4()));
        fs::create_dir_all(temporary.parent().unwrap_or(root))
            .map_err(|error| error.to_string())?;
        fs::write(
            &temporary,
            client
                .download_file(&format!("archives/{remote_folder}/{archive_name}"))
                .await
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if hash_file(&temporary)? != expected_hash {
            let _ = fs::remove_file(&temporary);
            return Err("远端快照校验失败，本地时间线未更改".into());
        }
        fs::rename(&temporary, entry_dir.join(archive_name)).map_err(|error| error.to_string())?;
        local_array.push(snapshot.clone());
    }
    local_array.sort_by_key(|item| {
        item.get("created_at_ms")
            .and_then(Value::as_u64)
            .unwrap_or_default()
    });
    let snapshot_count = local_array.len() as u64;
    let stored_bytes = local_array
        .iter()
        .filter_map(|item| item.get("size_bytes").and_then(Value::as_u64))
        .sum::<u64>();
    let last_snapshot = local_array
        .iter()
        .filter_map(|item| item.get("created_at_ms").and_then(Value::as_u64))
        .max()
        .unwrap_or_default();
    let mut catalog: Value = read_json(&root.join("catalog.json"))?;
    let summary = catalog
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .and_then(|entries| {
            entries
                .iter_mut()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(&local_entry.id))
        })
        .ok_or_else(|| "本地清单缺少存档".to_owned())?;
    summary["snapshot_count"] = Value::from(snapshot_count);
    summary["stored_bytes"] = Value::from(stored_bytes);
    summary["last_snapshot_at_ms"] = Value::from(last_snapshot);
    summary["snapshots"] = local_timeline;
    write_json_atomic(&root.join("catalog.json"), &catalog)
}

async fn github_upload_missing_snapshots(
    client: &GitHubClient,
    root: &Path,
    source_id: &str,
    entry_id: &str,
    remote_catalog: Option<Value>,
    remote_snapshot_ids: &[String],
) -> Result<(), String> {
    let local_catalog: Value = read_json(&root.join("catalog.json"))?;
    let entry: CatalogEntry = serde_json::from_value(
        local_catalog
            .get("entries")
            .and_then(Value::as_array)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.get("id").and_then(Value::as_str) == Some(entry_id))
            })
            .cloned()
            .ok_or_else(|| "本地存档不存在".to_owned())?,
    )
    .map_err(|error| error.to_string())?;
    let folder = checked_folder(&entry.folder)?;
    let local_dir = root.join("archives").join(folder);
    let mut changes = missing_snapshot_archives(&entry.snapshots, remote_snapshot_ids)?
        .into_iter()
        .map(|archive_name| {
            let path = local_dir.join(&archive_name);
            Ok(GitHubChange {
                path: format!("archives/{folder}/{archive_name}"),
                contents: Some(fs::read(path).map_err(|error| error.to_string())?),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut catalog = remote_catalog.unwrap_or_else(|| {
        serde_json::json!({ "format_version": 4, "updated_at_ms": unix_millis(), "categories": [], "entries": [] })
    });
    merge_catalog_entry(&mut catalog, &local_catalog, entry_id)?;
    catalog["updated_at_ms"] = Value::from(unix_millis());
    let library = updated_library(root)?;
    changes.push(GitHubChange {
        path: "catalog.json".into(),
        contents: Some(serde_json::to_vec_pretty(&catalog).map_err(|error| error.to_string())?),
    });
    changes.push(GitHubChange {
        path: "library.json".into(),
        contents: Some(serde_json::to_vec_pretty(&library).map_err(|error| error.to_string())?),
    });
    client
        .commit_changes(&format!("Sync Chronicle archive {}", entry.name), changes)
        .await
        .map_err(|error| error.to_string())?;
    save_sync_state(
        root,
        source_id,
        entry_id,
        &snapshot_ids_from_timeline(&Value::Array(entry.snapshots)),
    )
}

async fn github_sync_entry(
    client: &GitHubClient,
    root: &Path,
    source_id: &str,
    entry_id: &str,
) -> Result<SyncResultDto, String> {
    let local: Catalog = read_json(&root.join("catalog.json"))?;
    let remote_catalog = match client.get_json::<Value>("catalog.json").await {
        Ok(value) => Some(value),
        Err(GitHubError::NotFound(_)) => None,
        Err(error) => return Err(error.to_string()),
    };
    let remote: Option<Catalog> = remote_catalog
        .clone()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| error.to_string())?;
    let local_entry = local.entries.iter().find(|entry| entry.id == entry_id);
    let remote_entry = remote
        .as_ref()
        .and_then(|catalog| catalog.entries.iter().find(|entry| entry.id == entry_id));
    match (local_entry, remote_entry) {
        (Some(_), None) => {
            github_upload_missing_snapshots(client, root, source_id, entry_id, None, &[]).await?;
            Ok(SyncResultDto {
                status: "uploaded".into(),
                message: "已上传本地存档到 GitHub".into(),
            })
        }
        (None, Some(_)) => {
            github_overwrite_download(client, root, source_id, entry_id).await?;
            Ok(SyncResultDto {
                status: "downloaded".into(),
                message: "已从 GitHub 下载存档".into(),
            })
        }
        (Some(local_entry), Some(remote_entry)) => {
            let local_ids =
                snapshot_ids_from_timeline(&Value::Array(local_entry.snapshots.clone()));
            let remote_ids =
                snapshot_ids_from_timeline(&Value::Array(remote_entry.snapshots.clone()));
            let mut local_metadata =
                serde_json::to_value(local_entry).map_err(|error| error.to_string())?;
            let mut remote_metadata =
                serde_json::to_value(remote_entry).map_err(|error| error.to_string())?;
            for metadata in [&mut local_metadata, &mut remote_metadata] {
                metadata["snapshots"] = Value::Array(Vec::new());
                metadata["snapshot_count"] = Value::from(0);
                metadata["stored_bytes"] = Value::from(0);
                metadata["last_snapshot_at_ms"] = Value::Null;
            }
            if local_metadata != remote_metadata {
                return Ok(SyncResultDto {
                    status: "conflict".into(),
                    message: "本地和 GitHub 的存档设置不同，请选择覆盖方向".into(),
                });
            }
            let local_only = local_ids.iter().any(|id| !remote_ids.contains(id));
            let remote_only = remote_ids.iter().any(|id| !local_ids.contains(id));
            if local_only && remote_only {
                github_merge_remote_snapshots(client, root, local_entry, remote_entry).await?;
                github_upload_missing_snapshots(
                    client,
                    root,
                    source_id,
                    entry_id,
                    remote_catalog,
                    &remote_ids,
                )
                .await?;
                return Ok(SyncResultDto {
                    status: "uploaded".into(),
                    message: "已合并 GitHub 与本地互不冲突的时间节点".into(),
                });
            }
            if local_only {
                github_upload_missing_snapshots(
                    client,
                    root,
                    source_id,
                    entry_id,
                    remote_catalog,
                    &remote_ids,
                )
                .await?;
                return Ok(SyncResultDto {
                    status: "uploaded".into(),
                    message: "已上传 GitHub 缺少的时间节点".into(),
                });
            }
            if remote_only {
                github_overwrite_download(client, root, source_id, entry_id).await?;
                return Ok(SyncResultDto {
                    status: "downloaded".into(),
                    message: "已下载 GitHub 新增时间节点".into(),
                });
            }
            Ok(SyncResultDto {
                status: "current".into(),
                message: "本地与 GitHub 已经一致".into(),
            })
        }
        (None, None) => Err("本地和 GitHub 均找不到该存档".into()),
    }
}

async fn github_delete_entries(client: &GitHubClient, entry_ids: &[String]) -> Result<(), String> {
    let mut catalog: Value = client
        .get_json("catalog.json")
        .await
        .map_err(|error| error.to_string())?;
    let entries = catalog
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut changes = Vec::new();
    for entry in entries.iter().filter(|entry| {
        entry
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| entry_ids.iter().any(|candidate| candidate == id))
    }) {
        let folder = entry
            .get("folder")
            .and_then(Value::as_str)
            .ok_or_else(|| "远端存档缺少目录".to_owned())?;
        for snapshot in entry
            .get("snapshots")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let archive_name = snapshot
                .get("archive_name")
                .and_then(Value::as_str)
                .ok_or_else(|| "远端时间线缺少压缩包名称".to_owned())?;
            checked_folder(archive_name)?;
            changes.push(GitHubChange {
                path: format!("archives/{folder}/{archive_name}"),
                contents: None,
            });
        }
    }
    if let Some(entries) = catalog.get_mut("entries").and_then(Value::as_array_mut) {
        entries.retain(|entry| {
            !entry
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| entry_ids.iter().any(|candidate| candidate == id))
        });
    }
    changes.push(GitHubChange {
        path: "catalog.json".into(),
        contents: Some(serde_json::to_vec_pretty(&catalog).map_err(|error| error.to_string())?),
    });
    client
        .commit_changes("Delete Chronicle archives", changes)
        .await
        .map_err(|error| error.to_string())
}

async fn github_set_entry_sync_mode(
    client: &GitHubClient,
    entry_id: &str,
    sync_mode: &str,
) -> Result<(), String> {
    let mut catalog: Value = client
        .get_json("catalog.json")
        .await
        .map_err(|error| error.to_string())?;
    let entry = catalog
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .and_then(|entries| {
            entries
                .iter_mut()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(entry_id))
        })
        .ok_or_else(|| "远端存档不存在".to_owned())?;
    entry["sync_mode"] = Value::String(sync_mode.to_owned());
    client
        .commit_changes(
            "Update Chronicle sync mode",
            vec![GitHubChange {
                path: "catalog.json".into(),
                contents: Some(
                    serde_json::to_vec_pretty(&catalog).map_err(|error| error.to_string())?,
                ),
            }],
        )
        .await
        .map_err(|error| error.to_string())
}

fn merge_catalog_entry(
    remote_catalog: &mut Value,
    local_catalog: &Value,
    entry_id: &str,
) -> Result<(), String> {
    let entry = local_catalog
        .get("entries")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(entry_id))
        })
        .cloned()
        .ok_or_else(|| "本地存档不存在".to_owned())?;
    if !remote_catalog.is_object() {
        *remote_catalog = serde_json::json!({});
    }
    let remote = remote_catalog
        .as_object_mut()
        .ok_or_else(|| "远端清单格式无效".to_owned())?;
    let entries = remote
        .entry("entries")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "远端存档清单格式无效".to_owned())?;
    entries.retain(|existing| existing.get("id").and_then(Value::as_str) != Some(entry_id));
    entries.push(entry.clone());

    let mut required_categories = Vec::new();
    let local_categories = local_catalog
        .get("categories")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut parent = entry
        .get("category_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    while let Some(category_id) = parent {
        let Some(category) = local_categories
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(&category_id))
        else {
            break;
        };
        required_categories.push(category.clone());
        parent = category
            .get("parent_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
    }
    let categories = remote
        .entry("categories")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "远端分类清单格式无效".to_owned())?;
    for category in required_categories {
        let id = category.get("id").and_then(Value::as_str);
        categories.retain(|existing| existing.get("id").and_then(Value::as_str) != id);
        categories.push(category);
    }
    Ok(())
}

async fn upload_catalog_and_library(
    client: &RemoteStore,
    root: &Path,
    entry_id: &str,
) -> Result<(), String> {
    let local_catalog: Value = read_json(&root.join("catalog.json"))?;
    let mut catalog: Value = match client.get_json("catalog.json").await {
        Ok(value) => value,
        Err(WebDavError::NotFound(_)) => serde_json::json!({
            "format_version": local_catalog.get("format_version").cloned().unwrap_or(Value::from(3)),
            "updated_at_ms": unix_millis(),
            "categories": [],
            "entries": [],
        }),
        Err(error) => return Err(error.to_string()),
    };
    merge_catalog_entry(&mut catalog, &local_catalog, entry_id)?;
    catalog["updated_at_ms"] = Value::from(unix_millis());
    upload_catalog_and_library_value(client, root, &catalog).await
}

async fn upload_catalog_and_library_value(
    client: &RemoteStore,
    root: &Path,
    catalog: &Value,
) -> Result<(), String> {
    client
        .put_json("catalog.json", catalog)
        .await
        .map_err(|error| error.to_string())?;
    let library = updated_library(root)?;
    client
        .put_json("library.json", &library)
        .await
        .map_err(|error| error.to_string())
}

async fn upload_missing_webdav_snapshots(
    client: &RemoteStore,
    root: &Path,
    source_id: &str,
    entry_id: &str,
    remote_catalog: Option<Value>,
    remote_snapshot_ids: &[String],
    remote_entry_exists: bool,
) -> Result<(), String> {
    let local_catalog: Value = read_json(&root.join("catalog.json"))?;
    let entry: CatalogEntry = serde_json::from_value(
        local_catalog
            .get("entries")
            .and_then(Value::as_array)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.get("id").and_then(Value::as_str) == Some(entry_id))
            })
            .cloned()
            .ok_or_else(|| "本地存档不存在".to_owned())?,
    )
    .map_err(|error| error.to_string())?;
    let folder = checked_folder(&entry.folder)?;
    let remote_folder = format!("archives/{folder}");
    if !remote_entry_exists {
        if remote_catalog.is_none() {
            ensure_remote_layout(client).await?;
        }
        client
            .ensure_collection(&remote_folder)
            .await
            .map_err(|error| error.to_string())?;
    }
    let archives = missing_snapshot_archives(&entry.snapshots, remote_snapshot_ids)?;
    let local_dir = root.join("archives").join(folder);
    let upload_result: Result<(), String> = async {
        for archive_name in &archives {
            checked_folder(archive_name)?;
            if client.is_opendal() {
                match client.get_bytes(&format!("{remote_folder}/{archive_name}")).await {
                    Ok(existing) => {
                        if format!("{:x}", Sha256::digest(&existing)) != hash_file(&local_dir.join(archive_name))? {
                            return Err("远端已有同名快照且内容不同，未覆盖已有快照".into());
                        }
                        continue;
                    }
                    Err(WebDavError::NotFound(_)) => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
            client
                .upload_file(&format!("{remote_folder}/{archive_name}"), &local_dir.join(archive_name))
                .await
                .map_err(|error| error.to_string())?;
        }
        let mut catalog = remote_catalog.unwrap_or_else(|| {
            serde_json::json!({ "format_version": 4, "updated_at_ms": unix_millis(), "categories": [], "entries": [] })
        });
        merge_catalog_entry(&mut catalog, &local_catalog, entry_id)?;
        catalog["updated_at_ms"] = Value::from(unix_millis());
        upload_catalog_and_library_value(client, root, &catalog).await
    }
    .await;
    if let Err(error) = upload_result {
        // Keep immutable objects: a previous catalog or another device may reference them.
        return Err(error);
    }
    save_sync_state(
        root,
        source_id,
        entry_id,
        &snapshot_ids_from_timeline(&Value::Array(entry.snapshots)),
    )
}

#[tauri::command(async)]
pub async fn cloud_overwrite_upload(
    state: State<'_, AppState>,
    source_id: String,
    entry_id: String,
) -> Result<(), String> {
    let (configured_source, root, request_policy) = configured_source(&state, &source_id)?;
    if configured_source.provider == "legacy_github" {
        let client = github_client(
            &configured_source,
            credential(&configured_source)?,
            request_policy,
        )?;
        return github_overwrite_upload(&client, &root, &source_id, &entry_id).await;
    }
    let (client, _, root) = configured_client(&state, &source_id)?;
    if client.is_opendal() {
        let remote_catalog = match client.get_json::<Value>("catalog.json").await {
            Ok(value) => Some(value),
            Err(WebDavError::NotFound(_)) => None,
            Err(error) => return Err(error.to_string()),
        };
        // Immutable objects remain intact; catalog publication selects the replacement history.
        return upload_missing_webdav_snapshots(
            &client,
            &root,
            &source_id,
            &entry_id,
            remote_catalog,
            &[],
            false,
        )
        .await;
    }
    ensure_remote_layout(&client).await?;
    let catalog: Catalog = read_json(&root.join("catalog.json"))?;
    let entry = catalog
        .entries
        .iter()
        .find(|entry| entry.id == entry_id)
        .ok_or_else(|| "本地存档不存在".to_owned())?;
    let folder = checked_folder(&entry.folder)?;
    let local = root.join("archives").join(folder);
    let remote_folder = format!("archives/{folder}");
    let old_folder = format!(".chronicle-upload-old-{}", Uuid::new_v4());
    let had_remote = match client.move_object(&remote_folder, &old_folder).await {
        Ok(()) => true,
        Err(WebDavError::NotFound(_)) => false,
        Err(error) => return Err(error.to_string()),
    };
    let upload_result: Result<(), String> = async {
        client
            .ensure_collection(&remote_folder)
            .await
            .map_err(|error| error.to_string())?;
        for item in WalkDir::new(&local).min_depth(1) {
            let item = item.map_err(|error| error.to_string())?;
            if !item.file_type().is_file() {
                continue;
            }
            let relative = item
                .path()
                .strip_prefix(&local)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let remote = format!("{remote_folder}/{relative}");
            if relative == "entry.json" {
                let mut value: Value = read_json(item.path())?;
                if let Some(sources) = value.get_mut("sources").and_then(Value::as_array_mut) {
                    for source in sources {
                        source.as_object_mut().map(|object| object.remove("path"));
                    }
                }
                client
                    .put_json(&remote, &value)
                    .await
                    .map_err(|error| error.to_string())?;
            } else {
                client
                    .upload_file(&remote, item.path())
                    .await
                    .map_err(|error| error.to_string())?;
            }
        }
        upload_catalog_and_library(&client, &root, &entry_id).await
    }
    .await;
    if let Err(error) = upload_result {
        let _ = client.delete(&remote_folder).await;
        if had_remote {
            let _ = client.move_object(&old_folder, &remote_folder).await;
        }
        return Err(error);
    }
    if had_remote {
        client
            .delete(&old_folder)
            .await
            .map_err(|error| error.to_string())?;
    }
    let timeline = serde_json::to_value(&entry.snapshots).map_err(|error| error.to_string())?;
    save_sync_state(
        &root,
        &source_id,
        &entry_id,
        &snapshot_ids_from_timeline(&timeline),
    )?;
    Ok(())
}

#[tauri::command(async)]
#[allow(clippy::too_many_lines)]
pub async fn cloud_overwrite_download(
    state: State<'_, AppState>,
    source_id: String,
    entry_id: String,
) -> Result<(), String> {
    let (configured_source, root, request_policy) = configured_source(&state, &source_id)?;
    if configured_source.provider == "legacy_github" {
        let client = github_client(
            &configured_source,
            credential(&configured_source)?,
            request_policy,
        )?;
        return github_overwrite_download(&client, &root, &source_id, &entry_id).await;
    }
    let (client, _, root) = configured_client(&state, &source_id)?;
    let remote_catalog_value: Value = client
        .get_json("catalog.json")
        .await
        .map_err(|error| error.to_string())?;
    let remote_catalog: Catalog =
        serde_json::from_value(remote_catalog_value.clone()).map_err(|error| error.to_string())?;
    let remote_entry = remote_catalog
        .entries
        .iter()
        .find(|entry| entry.id == entry_id)
        .ok_or_else(|| "远端存档不存在".to_owned())?;
    let folder = checked_folder(&remote_entry.folder)?;
    let staging = root
        .join(".tmp")
        .join(format!("cloud-download-{}", Uuid::new_v4()));
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let timeline = Value::Array(remote_entry.snapshots.clone());
    let local_catalog_path = root.join("catalog.json");
    let mut local_catalog: Value = read_json(&local_catalog_path)?;
    if let Some(snapshots) = timeline.as_array() {
        for snapshot in snapshots {
            let archive_name = snapshot
                .get("archive_name")
                .and_then(Value::as_str)
                .ok_or_else(|| "远端时间线缺少压缩包名称".to_owned())?;
            checked_folder(archive_name)?;
            let expected_hash = snapshot
                .get("object_hash")
                .and_then(Value::as_str)
                .ok_or_else(|| "远端时间线缺少校验值".to_owned())?;
            let target = staging.join(archive_name);
            client
                .download_file(&format!("archives/{folder}/{archive_name}"), &target)
                .await
                .map_err(|error| error.to_string())?;
            if hash_file(&target)? != expected_hash {
                let _ = fs::remove_dir_all(&staging);
                return Err("远端快照校验失败，本地内容未更改".into());
            }
        }
    }
    let target = root.join("archives").join(folder);
    let backup = root
        .join(".tmp")
        .join(format!("cloud-old-{}", Uuid::new_v4()));
    if target.exists() {
        fs::rename(&target, &backup).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&staging, &target) {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(error.to_string());
    }
    let entries = local_catalog
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "本地清单格式无效".to_owned())?;
    entries.retain(|entry| entry.get("id").and_then(Value::as_str) != Some(&entry_id));
    let summary = remote_catalog_value
        .get("entries")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(&entry_id))
        })
        .cloned()
        .ok_or_else(|| "远端清单缺少存档".to_owned())?;
    entries.push(summary);
    if let Err(error) = write_json_atomic(&local_catalog_path, &local_catalog) {
        let _ = fs::remove_dir_all(&target);
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(error);
    }
    if backup.exists() {
        fs::remove_dir_all(backup).map_err(|error| error.to_string())?;
    }
    save_sync_state(
        &root,
        &source_id,
        &entry_id,
        &snapshot_ids_from_timeline(&timeline),
    )?;
    Ok(())
}

async fn merge_remote_snapshots(
    client: &RemoteStore,
    root: &Path,
    local_entry: &CatalogEntry,
    remote_entry: &CatalogEntry,
) -> Result<(), String> {
    let local_folder = checked_folder(&local_entry.folder)?;
    let remote_folder = checked_folder(&remote_entry.folder)?;
    let entry_dir = root.join("archives").join(local_folder);
    let mut local_timeline = Value::Array(local_entry.snapshots.clone());
    let remote_timeline = Value::Array(remote_entry.snapshots.clone());
    let local_ids = snapshot_ids_from_timeline(&local_timeline);
    let local_array = local_timeline
        .as_array_mut()
        .ok_or_else(|| "本地时间线格式无效".to_owned())?;
    for snapshot in remote_timeline
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| {
            !local_ids
                .iter()
                .any(|id| Some(id.as_str()) == item.get("id").and_then(Value::as_str))
        })
    {
        let archive_name = snapshot
            .get("archive_name")
            .and_then(Value::as_str)
            .ok_or_else(|| "远端时间线缺少压缩包名称".to_owned())?;
        checked_folder(archive_name)?;
        let expected_hash = snapshot
            .get("object_hash")
            .and_then(Value::as_str)
            .ok_or_else(|| "远端时间线缺少校验值".to_owned())?;
        let temporary = root.join(".tmp").join(format!("merge-{}", Uuid::new_v4()));
        client
            .download_file(
                &format!("archives/{remote_folder}/{archive_name}"),
                &temporary,
            )
            .await
            .map_err(|error| error.to_string())?;
        if hash_file(&temporary)? != expected_hash {
            let _ = fs::remove_file(&temporary);
            return Err("远端快照校验失败，本地时间线未更改".into());
        }
        fs::rename(&temporary, entry_dir.join(archive_name)).map_err(|error| error.to_string())?;
        local_array.push(snapshot.clone());
    }
    local_array.sort_by_key(|item| {
        item.get("created_at_ms")
            .and_then(Value::as_u64)
            .unwrap_or_default()
    });
    let snapshot_count = local_array.len() as u64;
    let stored_bytes = local_array
        .iter()
        .filter_map(|item| item.get("size_bytes").and_then(Value::as_u64))
        .sum::<u64>();
    let last_snapshot = local_array
        .iter()
        .filter_map(|item| item.get("created_at_ms").and_then(Value::as_u64))
        .max()
        .unwrap_or_default();
    let mut catalog: Value = read_json(&root.join("catalog.json"))?;
    if let Some(summary) = catalog
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .and_then(|entries| {
            entries
                .iter_mut()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(&local_entry.id))
        })
    {
        summary["snapshot_count"] = Value::from(snapshot_count);
        summary["stored_bytes"] = Value::from(stored_bytes);
        summary["last_snapshot_at_ms"] = Value::from(last_snapshot);
        summary["snapshots"] = local_timeline;
    }
    write_json_atomic(&root.join("catalog.json"), &catalog)
}

#[tauri::command(async)]
#[allow(clippy::too_many_lines)]
pub async fn cloud_sync_entry(
    state: State<'_, AppState>,
    source_id: String,
    entry_id: String,
) -> Result<SyncResultDto, String> {
    let (configured_source, root, request_policy) = configured_source(&state, &source_id)?;
    if configured_source.provider == "legacy_github" {
        let client = github_client(
            &configured_source,
            credential(&configured_source)?,
            request_policy,
        )?;
        return github_sync_entry(&client, &root, &source_id, &entry_id).await;
    }
    let (client, _, root) = configured_client(&state, &source_id)?;
    let local: Catalog = read_json(&root.join("catalog.json"))?;
    let remote_catalog = match client.get_json::<Value>("catalog.json").await {
        Ok(value) => Some(value),
        Err(WebDavError::NotFound(_)) => None,
        Err(error) => return Err(error.to_string()),
    };
    let remote: Option<Catalog> = remote_catalog
        .clone()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| error.to_string())?;
    let local_entry = local.entries.iter().find(|entry| entry.id == entry_id);
    let remote_entry = remote
        .as_ref()
        .and_then(|catalog| catalog.entries.iter().find(|entry| entry.id == entry_id));
    match (local_entry, remote_entry) {
        (Some(_), None) => {
            upload_missing_webdav_snapshots(
                &client,
                &root,
                &source_id,
                &entry_id,
                remote_catalog,
                &[],
                false,
            )
            .await?;
            Ok(SyncResultDto {
                status: "uploaded".into(),
                message: "已上传本地存档".into(),
            })
        }
        (None, Some(_)) => {
            cloud_overwrite_download(state, source_id, entry_id).await?;
            Ok(SyncResultDto {
                status: "downloaded".into(),
                message: "已下载远端存档".into(),
            })
        }
        (Some(local_entry), Some(remote_entry)) => {
            let local_timeline = Value::Array(local_entry.snapshots.clone());
            let local_ids = local_timeline
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect::<Vec<_>>();
            let remote_timeline = Value::Array(remote_entry.snapshots.clone());
            let remote_ids = snapshot_ids_from_timeline(&remote_timeline);
            let mut local_metadata =
                serde_json::to_value(local_entry).map_err(|error| error.to_string())?;
            let mut remote_metadata =
                serde_json::to_value(remote_entry).map_err(|error| error.to_string())?;
            for metadata in [&mut local_metadata, &mut remote_metadata] {
                metadata["snapshots"] = Value::Array(Vec::new());
                metadata["snapshot_count"] = Value::from(0);
                metadata["stored_bytes"] = Value::from(0);
                metadata["last_snapshot_at_ms"] = Value::Null;
            }
            if local_metadata != remote_metadata {
                return Ok(SyncResultDto {
                    status: "conflict".into(),
                    message: "本地和远端的存档设置不同，请选择覆盖方向".into(),
                });
            }
            let local_only = local_ids.iter().any(|id| !remote_ids.contains(id));
            let remote_only = remote_ids.iter().any(|id| !local_ids.contains(id));
            if local_only && remote_only {
                merge_remote_snapshots(&client, &root, local_entry, remote_entry).await?;
                upload_missing_webdav_snapshots(
                    &client,
                    &root,
                    &source_id,
                    &entry_id,
                    remote_catalog,
                    &remote_ids,
                    true,
                )
                .await?;
                return Ok(SyncResultDto {
                    status: "uploaded".into(),
                    message: "已合并两端互不冲突的时间节点".into(),
                });
            }
            if local_only {
                upload_missing_webdav_snapshots(
                    &client,
                    &root,
                    &source_id,
                    &entry_id,
                    remote_catalog,
                    &remote_ids,
                    true,
                )
                .await?;
                return Ok(SyncResultDto {
                    status: "uploaded".into(),
                    message: "已上传本地新增时间节点".into(),
                });
            }
            if remote_only {
                cloud_overwrite_download(state, source_id, entry_id).await?;
                return Ok(SyncResultDto {
                    status: "downloaded".into(),
                    message: "已下载远端新增时间节点".into(),
                });
            }
            Ok(SyncResultDto {
                status: "current".into(),
                message: "本地与远端已经一致".into(),
            })
        }
        (None, None) => Err("本地和远端均找不到该存档".into()),
    }
}

#[tauri::command(async)]
pub async fn cloud_delete_entries(
    state: State<'_, AppState>,
    source_id: String,
    entry_ids: Vec<String>,
) -> Result<(), String> {
    let (configured_source, _, request_policy) = configured_source(&state, &source_id)?;
    if configured_source.provider == "legacy_github" {
        let client = github_client(
            &configured_source,
            credential(&configured_source)?,
            request_policy,
        )?;
        return github_delete_entries(&client, &entry_ids).await;
    }
    let (client, _, _) = configured_client(&state, &source_id)?;
    let mut catalog_value: Value = client
        .get_json("catalog.json")
        .await
        .map_err(|error| error.to_string())?;
    let catalog: Catalog =
        serde_json::from_value(catalog_value.clone()).map_err(|error| error.to_string())?;
    let deletion_id = Uuid::new_v4().to_string();
    if client.is_opendal() {
        // Publish the reference removal first. Failed publication must never destroy archives.
        let folders: Vec<String> = catalog
            .entries
            .iter()
            .filter(|entry| entry_ids.contains(&entry.id))
            .map(|entry| checked_folder(&entry.folder).map(|folder| format!("archives/{folder}")))
            .collect::<Result<_, _>>()?;
        if let Some(entries) = catalog_value
            .get_mut("entries")
            .and_then(Value::as_array_mut)
        {
            entries.retain(|entry| {
                !entry
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| entry_ids.iter().any(|candidate| candidate == id))
            });
        }
        catalog_value["updated_at_ms"] = Value::from(unix_millis());
        client
            .put_json("catalog.json", &catalog_value)
            .await
            .map_err(|error| error.to_string())?;
        for folder in folders {
            client
                .delete(&folder)
                .await
                .map_err(|error| format!("已移除清单记录，但远端对象清理失败：{error}"))?;
        }
        return Ok(());
    }
    let mut moved = Vec::<(String, String)>::new();
    for entry in catalog
        .entries
        .iter()
        .filter(|entry| entry_ids.contains(&entry.id))
    {
        let folder = checked_folder(&entry.folder)?;
        let source = format!("archives/{folder}");
        let temporary = format!(".chronicle-delete-{deletion_id}-{folder}");
        if let Err(error) = client.move_object(&source, &temporary).await {
            for (old, staged) in moved.iter().rev() {
                let _ = client.move_object(staged, old).await;
            }
            return Err(error.to_string());
        }
        moved.push((source, temporary));
    }
    if let Some(entries) = catalog_value
        .get_mut("entries")
        .and_then(Value::as_array_mut)
    {
        entries.retain(|entry| {
            !entry
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| entry_ids.iter().any(|candidate| candidate == id))
        });
    }
    if let Err(error) = client.put_json("catalog.json", &catalog_value).await {
        for (old, staged) in moved.iter().rev() {
            let _ = client.move_object(staged, old).await;
        }
        return Err(error.to_string());
    }
    for (_, staged) in moved {
        client
            .delete(&staged)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command(async)]
pub async fn cloud_set_entry_sync_mode(
    state: State<'_, AppState>,
    source_id: String,
    entry_id: String,
    sync_mode: String,
) -> Result<(), String> {
    if !matches!(sync_mode.as_str(), "manual" | "automatic") {
        return Err("不支持的同步方式".into());
    }
    let (configured_source, _, request_policy) = configured_source(&state, &source_id)?;
    if configured_source.provider == "legacy_github" {
        let client = github_client(
            &configured_source,
            credential(&configured_source)?,
            request_policy,
        )?;
        github_set_entry_sync_mode(&client, &entry_id, &sync_mode).await?;
        let parsed_mode = if sync_mode == "automatic" {
            SyncMode::Automatic
        } else {
            SyncMode::Manual
        };
        state
            .repository
            .lock()
            .map_err(|_| storage_error())?
            .set_entry_sync_mode(&entry_id, parsed_mode)
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let (client, _, _) = configured_client(&state, &source_id)?;
    let mut catalog: Value = client
        .get_json("catalog.json")
        .await
        .map_err(|error| error.to_string())?;
    let entry = catalog
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .and_then(|entries| {
            entries
                .iter_mut()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(&entry_id))
        })
        .ok_or_else(|| "远端存档不存在".to_owned())?;
    entry["sync_mode"] = Value::String(sync_mode.clone());
    client
        .put_json("catalog.json", &catalog)
        .await
        .map_err(|error| error.to_string())?;
    let parsed_mode = if sync_mode == "automatic" {
        SyncMode::Automatic
    } else {
        SyncMode::Manual
    };
    state
        .repository
        .lock()
        .map_err(|_| storage_error())?
        .set_entry_sync_mode(&entry_id, parsed_mode)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{merge_catalog_entry, missing_snapshot_archives};

    #[test]
    fn opendal_verification_binds_secrets_and_target_but_not_map_order() {
        let mut source: super::CloudSourceInput = serde_json::from_value(json!({
            "id":"test", "name":"S3", "provider":"opendal", "endpoint":"", "username":"",
            "remotePath":"/Chronicle", "credentialRef":"test-ref", "scheme":"s3",
            "config":{"bucket":"test"}, "secretKeys":["secret_access_key","access_key_id"]
        }))
        .unwrap();
        let secrets = std::collections::HashMap::from([
            ("access_key_id".into(), "id".into()),
            ("secret_access_key".into(), "secret".into()),
        ]);
        let mut bundle = super::TestedCredential {
            fingerprint: super::source_fingerprint(&source, &secrets).unwrap(),
            secrets,
        };
        source.secret_keys.reverse();
        assert!(super::verify_bundle(&source, &bundle).is_ok());
        source.sync_enabled = true;
        assert!(super::verify_bundle(&source, &bundle).is_ok());
        source.remote_path = "/other".into();
        assert!(super::verify_bundle(&source, &bundle).is_err());
        source.remote_path = "/Chronicle".into();
        bundle
            .secrets
            .insert("secret_access_key".into(), "changed".into());
        assert!(super::verify_bundle(&source, &bundle).is_err());
    }

    #[test]
    fn old_source_routing_preserves_credential_branch_and_path() {
        let settings = json!({"cloud":{"sources":[{
            "id":"old", "name":"old", "provider":"github", "endpoint":"", "username":"",
            "repository":"owner/repo", "branch":"custom", "remotePath":"/Existing Root",
            "credentialRef":"existing-ref"
        }]}});
        let source = super::source_from_settings(&settings, "old").unwrap();
        assert_eq!(source.provider, "legacy_github");
        assert_eq!(source.branch, "custom");
        assert_eq!(source.remote_path, "/Existing Root");
        assert_eq!(source.credential_ref, "existing-ref");
    }

    #[test]
    fn rejects_plaintext_secrets_even_for_inactive_sources() {
        let settings = json!({"cloud":{"enabled":false, "sources":[{
            "id":"s3", "name":"S3", "provider":"opendal", "endpoint":"", "username":"",
            "remotePath":"/Chronicle", "credentialRef":"ref", "scheme":"s3",
            "config":{"secret_access_key":"must-not-save"}
        }]}});
        assert!(super::validate_settings(&settings, &json!({})).is_err());
        assert!(
            !super::validate_settings(&settings, &json!({}))
                .unwrap_err()
                .contains("must-not-save")
        );
    }

    #[test]
    fn accepts_paused_sources_without_a_legacy_active_source() {
        let settings = json!({"cloud":{"enabled":true, "sources":[{
            "id":"paused", "name":"Paused", "provider":"legacy_webdav", "endpoint":"", "username":"",
            "remotePath":"/Chronicle", "credentialRef":"ref", "syncEnabled":false
        }]}});
        assert!(super::validate_settings(&settings, &json!({})).is_ok());
    }

    #[test]
    fn scoped_catalog_upload_does_not_add_unsynced_local_entries() {
        let mut remote = json!({
            "entries": [{ "id": "remote", "name": "Other device" }],
            "categories": [],
        });
        let local = json!({
            "entries": [
                { "id": "selected", "name": "Selected", "category_id": "child" },
                { "id": "local-only", "name": "Not synced", "category_id": null }
            ],
            "categories": [
                { "id": "parent", "name": "Parent", "parent_id": null },
                { "id": "child", "name": "Child", "parent_id": "parent" }
            ]
        });

        merge_catalog_entry(&mut remote, &local, "selected").unwrap();

        let entries = remote["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry| entry["id"] == "remote"));
        assert!(entries.iter().any(|entry| entry["id"] == "selected"));
        assert!(!entries.iter().any(|entry| entry["id"] == "local-only"));
        assert_eq!(remote["categories"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn upload_plan_contains_only_snapshots_missing_from_remote() {
        let local = vec![
            json!({ "id": "already-there", "archive_name": "old.7z" }),
            json!({ "id": "new", "archive_name": "new.7z" }),
        ];

        assert_eq!(
            missing_snapshot_archives(&local, &["already-there".into()]).unwrap(),
            vec!["new.7z"]
        );
    }
}
