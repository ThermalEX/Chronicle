#![allow(clippy::needless_pass_by_value)]

use std::{
    fs,
    io::{BufReader, Read, Write},
    path::{Component, Path, PathBuf},
};

use chronicle_core::SyncMode;
use chronicle_sync::{RequestPolicy, WebDavClient, WebDavError, WebDavSource};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::AppState;

const CREDENTIAL_SERVICE: &str = "Chronicle WebDAV";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSourceInput {
    id: String,
    name: String,
    provider: String,
    endpoint: String,
    username: String,
    remote_path: String,
    credential_ref: String,
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
        .and_then(|source| serde_json::from_value(source).map_err(|error| error.to_string()))
}

fn credential(source: &CloudSourceInput) -> Result<String, String> {
    let account = if source.credential_ref.is_empty() {
        &source.id
    } else {
        &source.credential_ref
    };
    keyring::Entry::new(CREDENTIAL_SERVICE, account)
        .map_err(|error| error.to_string())?
        .get_password()
        .map_err(|_| "该同步源尚未保存密码".to_owned())
}

fn client(
    source: &CloudSourceInput,
    password: String,
    request_policy: RequestPolicy,
) -> Result<WebDavClient, String> {
    if source.provider != "webdav" {
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
    .map_err(|error| error.to_string())
}

fn checked_folder(value: &str) -> Result<&str, String> {
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

fn sanitize_entry(mut entry: Value) -> Value {
    if let Some(sources) = entry.get_mut("sources").and_then(Value::as_array_mut) {
        for source in sources {
            source.as_object_mut().map(|object| object.remove("path"));
        }
    }
    entry
        .as_object_mut()
        .map(|object| object.remove("sync_mode"));
    entry
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

fn configured_client(
    state: &State<'_, AppState>,
    source_id: &str,
) -> Result<(WebDavClient, CloudSourceInput, PathBuf), String> {
    let repository = state.repository.lock().map_err(|_| storage_error())?;
    let settings = repository
        .load_settings()
        .map_err(|error| error.to_string())?;
    let root = repository.root().to_path_buf();
    drop(repository);
    let source = source_from_settings(&settings, source_id)?;
    let client = client(&source, credential(&source)?, policy(&settings))?;
    Ok((client, source, root))
}

#[tauri::command(async)]
pub async fn save_cloud_credential(
    source_id: String,
    credential_ref: String,
    password: String,
) -> Result<(), String> {
    let account = if credential_ref.is_empty() {
        source_id.as_str()
    } else {
        credential_ref.as_str()
    };
    let entry =
        keyring::Entry::new(CREDENTIAL_SERVICE, account).map_err(|error| error.to_string())?;
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
    let password = if password.is_empty() {
        credential(&source)?
    } else {
        password
    };
    let test_client = client(&source, password, RequestPolicy::default())?;
    test_client
        .test_capabilities()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub async fn cloud_preview(
    state: State<'_, AppState>,
    source_id: String,
) -> Result<CloudPreviewDto, String> {
    let (client, source, root) = configured_client(&state, &source_id)?;
    let library = ensure_remote_library(&client, &root).await?;
    let catalog: Option<Catalog> = match client.get_json("data/catalog.json").await {
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
            updated_at: library
                .get("updatedAtMs")
                .and_then(Value::as_u64),
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

async fn ensure_remote_layout(client: &WebDavClient) -> Result<(), String> {
    for path in ["", "config", "data", "data/entries"] {
        client
            .ensure_collection(path)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn ensure_remote_library(client: &WebDavClient, root: &Path) -> Result<Value, String> {
    match client.get_json("config/library.json").await {
        Ok(value) => Ok(value),
        Err(WebDavError::NotFound(_)) => {
            ensure_remote_layout(client).await?;
            let library_path = root.join("config/library.json");
            let mut library = if library_path.is_file() {
                read_json::<Value>(&library_path)?
            } else {
                serde_json::json!({ "formatVersion": 1, "libraryId": Uuid::new_v4().to_string() })
            };
            library["updatedAtMs"] = Value::from(unix_millis());
            write_json_atomic(&library_path, &library)?;
            client
                .put_json("config/library.json", &library)
                .await
                .map_err(|error| error.to_string())?;
            Ok(library)
        }
        Err(error) => Err(error.to_string()),
    }
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
    client: &WebDavClient,
    root: &Path,
    entry_id: &str,
) -> Result<(), String> {
    let local_catalog: Value = read_json(&root.join("data/catalog.json"))?;
    let mut catalog: Value = match client.get_json("data/catalog.json").await {
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
    client
        .put_json("data/catalog.json", &catalog)
        .await
        .map_err(|error| error.to_string())?;
    let library_path = root.join("config/library.json");
    let mut library = if library_path.is_file() {
        read_json::<Value>(&library_path)?
    } else {
        serde_json::json!({ "formatVersion": 1, "libraryId": Uuid::new_v4().to_string() })
    };
    library["updatedAtMs"] = Value::from(unix_millis());
    write_json_atomic(&library_path, &library)?;
    client
        .put_json("config/library.json", &library)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub async fn cloud_overwrite_upload(
    state: State<'_, AppState>,
    source_id: String,
    entry_id: String,
) -> Result<(), String> {
    let (client, _, root) = configured_client(&state, &source_id)?;
    ensure_remote_layout(&client).await?;
    let catalog: Catalog = read_json(&root.join("data/catalog.json"))?;
    let entry = catalog
        .entries
        .iter()
        .find(|entry| entry.id == entry_id)
        .ok_or_else(|| "本地存档不存在".to_owned())?;
    let folder = checked_folder(&entry.folder)?;
    let local = root.join("data/entries").join(folder);
    let remote_folder = format!("data/entries/{folder}");
    let old_folder = format!("data/.chronicle-upload-old-{}", Uuid::new_v4());
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
    let timeline: Value = read_json(&local.join("timeline.json"))?;
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
    let (client, _, root) = configured_client(&state, &source_id)?;
    let remote_catalog_value: Value = client
        .get_json("data/catalog.json")
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
        .join("data/.tmp")
        .join(format!("cloud-download-{}", Uuid::new_v4()));
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let mut entry_value: Value = client
        .get_json(&format!("data/entries/{folder}/entry.json"))
        .await
        .map_err(|error| error.to_string())?;
    let timeline: Value = client
        .get_json(&format!("data/entries/{folder}/timeline.json"))
        .await
        .map_err(|error| error.to_string())?;
    let local_catalog_path = root.join("data/catalog.json");
    let mut local_catalog: Value = read_json(&local_catalog_path)?;
    if let Some(local_summary) = local_catalog
        .get("entries")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(&entry_id))
        })
        && let Some(local_folder) = local_summary.get("folder").and_then(Value::as_str)
        && let Ok(local_entry) = read_json::<Value>(
            &root
                .join("data/entries")
                .join(local_folder)
                .join("entry.json"),
        )
        && let (Some(remote_sources), Some(local_sources)) = (
            entry_value.get_mut("sources").and_then(Value::as_array_mut),
            local_entry.get("sources").and_then(Value::as_array),
        )
    {
        for remote in remote_sources {
            let id = remote.get("id").and_then(Value::as_str);
            if let Some(path) = local_sources
                .iter()
                .find(|source| source.get("id").and_then(Value::as_str) == id)
                .and_then(|source| source.get("path"))
                .cloned()
            {
                remote["path"] = path;
            }
        }
    }
    write_json_atomic(&staging.join("entry.json"), &entry_value)?;
    write_json_atomic(&staging.join("timeline.json"), &timeline)?;
    if let Some(snapshots) = timeline.as_array() {
        for snapshot in snapshots {
            let archive_name = snapshot
                .get("archive_name")
                .and_then(Value::as_str)
                .ok_or_else(|| "远端时间线缺少压缩包名称".to_owned())?;
            let expected_hash = snapshot
                .get("object_hash")
                .and_then(Value::as_str)
                .ok_or_else(|| "远端时间线缺少校验值".to_owned())?;
            let target = staging.join(archive_name);
            client
                .download_file(&format!("data/entries/{folder}/{archive_name}"), &target)
                .await
                .map_err(|error| error.to_string())?;
            if hash_file(&target)? != expected_hash {
                let _ = fs::remove_dir_all(&staging);
                return Err("远端快照校验失败，本地内容未更改".into());
            }
        }
    }
    let target = root.join("data/entries").join(folder);
    let backup = root
        .join("data/.tmp")
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

async fn snapshot_ids(client: &WebDavClient, folder: &str) -> Result<Vec<String>, String> {
    let timeline: Value = client
        .get_json(&format!("data/entries/{folder}/timeline.json"))
        .await
        .map_err(|error| error.to_string())?;
    Ok(timeline
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect())
}

async fn merge_remote_snapshots(
    client: &WebDavClient,
    root: &Path,
    local_entry: &CatalogEntry,
    remote_entry: &CatalogEntry,
) -> Result<(), String> {
    let local_folder = checked_folder(&local_entry.folder)?;
    let remote_folder = checked_folder(&remote_entry.folder)?;
    let entry_dir = root.join("data/entries").join(local_folder);
    let timeline_path = entry_dir.join("timeline.json");
    let mut local_timeline: Value = read_json(&timeline_path)?;
    let remote_timeline: Value = client
        .get_json(&format!("data/entries/{remote_folder}/timeline.json"))
        .await
        .map_err(|error| error.to_string())?;
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
        let expected_hash = snapshot
            .get("object_hash")
            .and_then(Value::as_str)
            .ok_or_else(|| "远端时间线缺少校验值".to_owned())?;
        let temporary = root
            .join("data/.tmp")
            .join(format!("merge-{}", Uuid::new_v4()));
        client
            .download_file(
                &format!("data/entries/{remote_folder}/{archive_name}"),
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
    write_json_atomic(&timeline_path, &local_timeline)?;
    let mut catalog: Value = read_json(&root.join("data/catalog.json"))?;
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
    }
    write_json_atomic(&root.join("data/catalog.json"), &catalog)
}

#[tauri::command(async)]
pub async fn cloud_sync_entry(
    state: State<'_, AppState>,
    source_id: String,
    entry_id: String,
) -> Result<SyncResultDto, String> {
    let (client, _, root) = configured_client(&state, &source_id)?;
    let local: Catalog = read_json(&root.join("data/catalog.json"))?;
    let remote: Option<Catalog> = match client.get_json("data/catalog.json").await {
        Ok(value) => Some(value),
        Err(WebDavError::NotFound(_)) => None,
        Err(error) => return Err(error.to_string()),
    };
    let local_entry = local.entries.iter().find(|entry| entry.id == entry_id);
    let remote_entry = remote
        .as_ref()
        .and_then(|catalog| catalog.entries.iter().find(|entry| entry.id == entry_id));
    match (local_entry, remote_entry) {
        (Some(_), None) => {
            cloud_overwrite_upload(state, source_id, entry_id).await?;
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
            let local_timeline: Value = read_json(
                &root
                    .join("data/entries")
                    .join(checked_folder(&local_entry.folder)?)
                    .join("timeline.json"),
            )?;
            let local_ids = local_timeline
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect::<Vec<_>>();
            let remote_ids = snapshot_ids(&client, checked_folder(&remote_entry.folder)?).await?;
            let local_metadata = sanitize_entry(read_json(
                &root
                    .join("data/entries")
                    .join(checked_folder(&local_entry.folder)?)
                    .join("entry.json"),
            )?);
            let remote_metadata: Value = client
                .get_json(&format!(
                    "data/entries/{}/entry.json",
                    checked_folder(&remote_entry.folder)?
                ))
                .await
                .map_err(|error| error.to_string())?;
            if local_metadata != sanitize_entry(remote_metadata) {
                return Ok(SyncResultDto {
                    status: "conflict".into(),
                    message: "本地和远端的存档设置不同，请选择覆盖方向".into(),
                });
            }
            let local_only = local_ids.iter().any(|id| !remote_ids.contains(id));
            let remote_only = remote_ids.iter().any(|id| !local_ids.contains(id));
            if local_only && remote_only {
                merge_remote_snapshots(&client, &root, local_entry, remote_entry).await?;
                cloud_overwrite_upload(state, source_id, entry_id).await?;
                return Ok(SyncResultDto {
                    status: "uploaded".into(),
                    message: "已合并两端互不冲突的时间节点".into(),
                });
            }
            if local_only {
                cloud_overwrite_upload(state, source_id, entry_id).await?;
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
    let (client, _, _) = configured_client(&state, &source_id)?;
    let mut catalog_value: Value = client
        .get_json("data/catalog.json")
        .await
        .map_err(|error| error.to_string())?;
    let catalog: Catalog =
        serde_json::from_value(catalog_value.clone()).map_err(|error| error.to_string())?;
    let deletion_id = Uuid::new_v4().to_string();
    let mut moved = Vec::<(String, String)>::new();
    for entry in catalog
        .entries
        .iter()
        .filter(|entry| entry_ids.contains(&entry.id))
    {
        let folder = checked_folder(&entry.folder)?;
        let source = format!("data/entries/{folder}");
        let temporary = format!("data/.chronicle-delete-{deletion_id}-{folder}");
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
    if let Err(error) = client.put_json("data/catalog.json", &catalog_value).await {
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
    let (client, _, _) = configured_client(&state, &source_id)?;
    let mut catalog: Value = client
        .get_json("data/catalog.json")
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
        .put_json("data/catalog.json", &catalog)
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

    use super::merge_catalog_entry;

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
}
