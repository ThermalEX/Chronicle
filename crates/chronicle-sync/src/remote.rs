//! Root-confined remote I/O shared by the legacy WebDAV and OpenDAL protocols.
use crate::{RequestPolicy, WebDavClient, WebDavError};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use opendal::{
    ErrorKind, Operator,
    layers::{ConcurrentLimitLayer, RetryLayer, TimeoutLayer},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::HashMap, path::Path, time::Duration};

pub const OPEN_DAL_SCHEMES: &[&str] = &[
    "github", "webdav", "s3", "b2", "azblob", "gcs", "oss", "cos", "obs", "tos", "swift",
];
// Unknown advanced options are secrets, including future service options.
pub const PUBLIC_CONFIG_KEYS: &[&str] = &[
    "endpoint",
    "bucket",
    "bucket_id",
    "region",
    "container",
    "owner",
    "repo",
    "username",
    "account_name",
];

pub struct OpenDalSource {
    pub scheme: String,
    pub root: String,
    pub config: HashMap<String, String>,
    pub secret_keys: Vec<String>,
}

type Result<T> = std::result::Result<T, WebDavError>;
fn invalid(message: &str) -> WebDavError {
    WebDavError::Remote(message.into())
}

/// Validate before OpenDAL can normalize a path or an HTTP server can decode it.
pub fn validate_relative_path(path: &str) -> Result<()> {
    if path.starts_with('/')
        || path.contains(['\\', ':', '\0'])
        || path.split('/').any(|part| part == ".." || part == ".")
    {
        return Err(invalid("远端路径必须位于同步源目录内"));
    }
    Ok(())
}

fn remote_error(error: opendal::Error) -> WebDavError {
    // OpenDAL's full error chain may contain credentials and request URLs.
    if error.kind() == ErrorKind::NotFound {
        WebDavError::NotFound("远端对象".into())
    } else {
        invalid(&format!("OpenDAL 操作失败（{:?}）", error.kind()))
    }
}

fn is_local_credential_key(key: &str) -> bool {
    matches!(
        key,
        "root"
            | "branch"
            | "credential_path"
            | "google_application_credentials"
            | "aws_profile"
    ) || key.ends_with("_path")
        || key.ends_with("_file")
}

fn gcs_credential_as_base64(value: &str) -> Result<String> {
    let json: serde_json::Value = serde_json::from_str(value)
        .map_err(|_| invalid("GCS credential 必须是服务账号 JSON 内容"))?;
    serde_json::to_vec(&json)
        .map(|bytes| STANDARD.encode(bytes))
        .map_err(WebDavError::from)
}

/// Dispatches the shared catalog/archive protocol without changing legacy MOVE semantics.
#[derive(Clone)]
pub enum RemoteStore {
    LegacyWebDav(WebDavClient),
    OpenDal(
        Operator,
        std::sync::Arc<tokio::sync::Mutex<tokio::time::Instant>>,
        Duration,
    ),
}

impl RemoteStore {
    pub fn opendal(
        source: OpenDalSource,
        secrets: HashMap<String, String>,
        policy: RequestPolicy,
    ) -> Result<Self> {
        // Also initialize explicitly for Tauri staticlib linkage, where constructors may be dropped.
        opendal::install_default();
        if !OPEN_DAL_SCHEMES.contains(&source.scheme.as_str()) {
            return Err(invalid("未编译或不允许的 OpenDAL 服务"));
        }
        validate_relative_path(source.root.trim_matches('/'))?;
        if source.config.keys().any(|key| {
            !PUBLIC_CONFIG_KEYS.contains(&key.as_str()) || source.secret_keys.contains(key)
        }) {
            return Err(invalid("机密配置不能写入公开设置"));
        }
        if let Some(endpoint) = source
            .config
            .get("endpoint")
            .filter(|value| !value.is_empty())
        {
            let url = reqwest::Url::parse(endpoint)
                .map_err(|_| invalid("endpoint 必须是有效 HTTP(S) 地址"))?;
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(invalid("公开 endpoint 不得包含账号密码、查询令牌或片段"));
            }
        }
        if source
            .secret_keys
            .iter()
            .any(|key| !secrets.contains_key(key))
        {
            return Err(invalid("缺少 OpenDAL 机密字段"));
        }
        let scheme = source.scheme;
        let mut config = source.config;
        for key in source.secret_keys {
            if is_local_credential_key(&key) {
                return Err(invalid("不允许覆盖 root、分支或读取本地凭据文件"));
            }
            config.insert(key.clone(), secrets[&key].clone());
        }
        if scheme == "gcs" {
            // This app only accepts a credential stored in the Windows keyring.
            // Do not let OpenDAL fall back to environment, well-known, file or VM credentials.
            config.insert("disable_config_load".into(), "true".into());
            config.insert("disable_vm_metadata".into(), "true".into());
            if let Some(value) = config.get("credential").cloned() {
                config.insert("credential".into(), gcs_credential_as_base64(&value)?);
            }
        }
        config.insert("root".into(), format!("/{}", source.root.trim_matches('/')));
        let operator = Operator::via_iter(&scheme, config)
            .map_err(remote_error)?
            .layer(
                TimeoutLayer::new()
                    .with_timeout(Duration::from_secs(60))
                    .with_io_timeout(Duration::from_secs(300)),
            )
            .layer(
                RetryLayer::new()
                    .with_max_times(usize::from(policy.retry_limit.min(10)))
                    .with_min_delay(Duration::from_millis(policy.base_retry_delay_ms))
                    .with_max_delay(Duration::from_millis(policy.max_retry_delay_ms)),
            )
            .layer(ConcurrentLimitLayer::new(
                policy
                    .max_concurrent_transfers
                    .min(policy.max_concurrent_metadata_reads)
                    .clamp(1, 4),
            ));
        let store = Self::OpenDal(
            operator,
            std::sync::Arc::new(tokio::sync::Mutex::new(tokio::time::Instant::now())),
            Duration::from_millis(policy.request_delay_ms.min(5000)),
        );
        store.require_capabilities()?;
        Ok(store)
    }

    async fn pace(&self) {
        if let Self::OpenDal(_, next, delay) = self {
            let mut next = next.lock().await;
            tokio::time::sleep_until(*next).await;
            *next = tokio::time::Instant::now() + *delay;
        }
    }

    pub fn is_opendal(&self) -> bool {
        matches!(self, Self::OpenDal(..))
    }

    pub fn require_capabilities(&self) -> Result<()> {
        if let Self::OpenDal(op, ..) = self {
            let caps = op.info().capability();
            if !(caps.read && caps.write && caps.list && caps.delete) {
                return Err(invalid("该服务缺少读取、写入、列举或删除能力"));
            }
        }
        Ok(())
    }

    pub async fn ensure_collection(&self, path: &str) -> Result<()> {
        self.pace().await;
        validate_relative_path(path)?;
        match self {
            Self::LegacyWebDav(client) => client.ensure_collection(path).await,
            Self::OpenDal(op, ..) if op.info().capability().create_dir && !path.is_empty() => op
                .create_dir(&format!("{}/", path.trim_end_matches('/')))
                .await
                .map_err(remote_error),
            Self::OpenDal(..) => Ok(()),
        }
    }
    pub async fn get_bytes(&self, path: &str) -> Result<Vec<u8>> {
        self.pace().await;
        validate_relative_path(path)?;
        match self {
            Self::LegacyWebDav(client) => client.get_bytes(path).await,
            Self::OpenDal(op, ..) => op
                .read(path)
                .await
                .map(|buffer| buffer.to_vec())
                .map_err(remote_error),
        }
    }
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        Ok(serde_json::from_slice(&self.get_bytes(path).await?)?)
    }
    pub async fn put_atomic(&self, path: &str, bytes: Vec<u8>) -> Result<()> {
        self.pace().await;
        validate_relative_path(path)?;
        match self {
            Self::LegacyWebDav(client) => client.put_atomic(path, bytes).await,
            Self::OpenDal(op, ..) => op
                .write(path, bytes)
                .await
                .map(|_| ())
                .map_err(remote_error),
        }
    }
    pub async fn put_json<T: Serialize>(&self, path: &str, value: &T) -> Result<()> {
        self.put_atomic(path, serde_json::to_vec_pretty(value)?)
            .await
    }
    pub async fn upload_file(&self, path: &str, local: &Path) -> Result<()> {
        self.put_atomic(path, tokio::fs::read(local).await?).await
    }
    pub async fn download_file(&self, path: &str, local: &Path) -> Result<()> {
        let bytes = self.get_bytes(path).await?;
        if let Some(parent) = local.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(local, bytes).await?;
        Ok(())
    }
    /// Recursive deletion is always confined to an explicit directory prefix.
    pub async fn delete(&self, path: &str) -> Result<()> {
        self.pace().await;
        validate_relative_path(path)?;
        if path.trim_matches('/').is_empty() {
            return Err(invalid("不能删除同步源根目录"));
        }
        match self {
            Self::LegacyWebDav(client) => client.delete(path).await,
            Self::OpenDal(op, ..) => {
                let prefix = format!("{}/", path.trim_end_matches('/'));
                op.delete_with(&prefix)
                    .recursive(true)
                    .await
                    .map_err(remote_error)?;
                op.delete(path).await.map_err(remote_error)
            }
        }
    }
    pub async fn move_object(&self, source: &str, destination: &str) -> Result<()> {
        self.pace().await;
        validate_relative_path(source)?;
        validate_relative_path(destination)?;
        match self {
            Self::LegacyWebDav(client) => client.move_object(source, destination).await,
            Self::OpenDal(op, ..) if op.info().capability().rename => {
                op.rename(source, destination).await.map_err(remote_error)
            }
            Self::OpenDal(..) => Err(invalid("服务不支持重命名")),
        }
    }
    pub async fn test_capabilities(&self) -> Result<()> {
        self.require_capabilities()?;
        let Self::OpenDal(op, ..) = self else {
            if let Self::LegacyWebDav(client) = self {
                return client.test_capabilities().await;
            }
            unreachable!()
        };
        let prefix = format!(".chronicle-probe-{}/", uuid::Uuid::new_v4());
        let path = format!("{prefix}probe");
        let bytes = b"chronicle-capability-probe".to_vec();
        let result = async {
            self.ensure_collection(prefix.trim_end_matches('/')).await?;
            self.put_atomic(&path, bytes.clone()).await?;
            if self.get_bytes(&path).await? != bytes {
                return Err(invalid("测试对象读回不一致"));
            }
            let entries = op.list(&prefix).await.map_err(remote_error)?;
            if !entries.iter().any(|entry| entry.path() == path) {
                return Err(invalid("测试对象列举失败"));
            }
            Ok(())
        }
        .await;
        // Cleanup is attempted even after a failed write/read/list. Its failure blocks activation.
        op.delete_with(&prefix)
            .recursive(true)
            .await
            .map_err(|_| invalid("测试目录清理失败，不能启用此同步源"))?;
        let remaining = match op.list(&prefix).await {
            Ok(entries) => !entries.is_empty(),
            Err(error) if error.kind() == ErrorKind::NotFound => false,
            Err(error) => return Err(remote_error(error)),
        };
        let exists = match op.read(&path).await {
            Ok(_) => true,
            Err(error) if error.kind() == ErrorKind::NotFound => false,
            Err(error) => return Err(remote_error(error)),
        };
        if exists || remaining {
            return Err(invalid("测试目录清理未确认，不能启用此同步源"));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_paths_and_uncompiled_services() {
        for path in ["/escape", "../escape", "a/../b", "a\\b", "C:/x"] {
            assert!(validate_relative_path(path).is_err());
        }
        assert!(validate_relative_path("archives/name/snapshot.7z").is_ok());
        let source = OpenDalSource {
            scheme: "fs".into(),
            root: "Chronicle".into(),
            config: HashMap::new(),
            secret_keys: vec![],
        };
        assert!(RemoteStore::opendal(source, HashMap::new(), RequestPolicy::default()).is_err());
    }

    #[test]
    fn gcs_credentials_are_encoded_and_local_credential_aliases_are_rejected() {
        let encoded = gcs_credential_as_base64(r#"{"client_email":"a@example.test","private_key":"key"}"#).unwrap();
        let decoded = STANDARD.decode(encoded).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(value["client_email"], "a@example.test");
        assert!(gcs_credential_as_base64("not-json").is_err());
        for key in ["credential_path", "google_application_credentials", "service_account_path", "aws_profile"] {
            assert!(is_local_credential_key(key), "{key}");
        }
    }
    #[tokio::test]
    async fn memory_probe_and_recursive_delete_preserve_sibling_and_catalog() {
        let op = Operator::new(opendal::services::Memory::default()).unwrap();
        let store = RemoteStore::OpenDal(
            op.clone(),
            std::sync::Arc::new(tokio::sync::Mutex::new(tokio::time::Instant::now())),
            Duration::ZERO,
        );
        store
            .put_atomic("catalog.json", b"unchanged".to_vec())
            .await
            .unwrap();
        store.test_capabilities().await.unwrap();
        assert_eq!(op.list("").await.unwrap().len(), 1);
        store.put_atomic("archives/a/one", vec![1]).await.unwrap();
        store
            .put_atomic("archives/a/nested/two", vec![2])
            .await
            .unwrap();
        store.put_atomic("archives/ab/keep", vec![3]).await.unwrap();
        store.delete("archives/a").await.unwrap();
        assert!(!op.exists("archives/a/nested/two").await.unwrap());
        assert_eq!(store.get_bytes("archives/ab/keep").await.unwrap(), vec![3]);
        assert_eq!(store.get_bytes("catalog.json").await.unwrap(), b"unchanged");
        assert!(store.delete("").await.is_err());
    }

    #[tokio::test]
    async fn permanent_http_errors_are_not_retried_and_secrets_are_redacted() {
        use std::io::{Read, Write};
        use std::sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let worker_stop = stop.clone();
        let worker_requests = requests.clone();
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::SeqCst) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = [0; 8192];
                    let _ = stream.read(&mut bytes);
                    worker_requests.fetch_add(1, Ordering::SeqCst);
                    let body =
                        "<Error><Code>AccessDenied</Code><Message>secret-value</Message></Error>";
                    write!(stream, "HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        });
        let source = OpenDalSource {
            scheme: "s3".into(),
            root: "Chronicle".into(),
            config: HashMap::from([
                ("endpoint".into(), format!("http://{address}")),
                ("bucket".into(), "test".into()),
                ("region".into(), "us-east-1".into()),
            ]),
            secret_keys: vec!["access_key_id".into(), "secret_access_key".into()],
        };
        let store = RemoteStore::opendal(
            source,
            HashMap::from([
                ("access_key_id".into(), "test-id".into()),
                ("secret_access_key".into(), "secret-value".into()),
            ]),
            RequestPolicy {
                request_delay_ms: 0,
                base_retry_delay_ms: 1,
                max_retry_delay_ms: 2,
                ..RequestPolicy::default()
            },
        )
        .unwrap();
        let error = store
            .get_bytes("catalog.json")
            .await
            .unwrap_err()
            .to_string();
        stop.store(true, Ordering::SeqCst);
        worker.join().unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 1, "{error}");
        assert!(!error.contains("secret-value"));
        assert!(!error.contains("test-id"));
        assert!(error.contains("PermissionDenied"));
    }
}
