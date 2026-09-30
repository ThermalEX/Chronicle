#![allow(clippy::missing_errors_doc)]

use std::{path::Path, time::Duration};

use reqwest::{Client, Method, StatusCode, header};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;
use tokio::time::sleep;
use uuid::Uuid;

use crate::RequestPolicy;

/// Connection details for one `WebDAV` repository.
#[derive(Clone, Debug)]
pub struct WebDavSource {
    pub endpoint: String,
    pub username: String,
    pub password: String,
    pub remote_path: String,
}

#[cfg(test)]
mod tests {
    use super::{WebDavClient, WebDavSource, requires_delete_before_overwrite};
    use crate::RequestPolicy;
    #[tokio::test]
    async fn nested_collections_create_parents_first_and_can_be_repeated() {
        use std::{
            collections::HashSet,
            io::{Read, Write},
            net::TcpListener,
            time::Instant,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let mut directories = HashSet::from(["/Chronicle".to_owned()]);
            let mut paths = Vec::new();
            let deadline = Instant::now() + std::time::Duration::from_secs(5);
            while paths.len() < 6 && Instant::now() < deadline {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                };
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(1)))
                    .unwrap();
                let mut bytes = [0; 4096];
                let count = socket.read(&mut bytes).unwrap();
                let request = String::from_utf8_lossy(&bytes[..count]);
                let path = request
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .trim_end_matches('/')
                    .to_owned();
                assert!(request.starts_with("MKCOL "));
                let parent = path.rsplit_once('/').unwrap().0;
                let status = if directories.contains(&path) {
                    "405 Method Not Allowed"
                } else if !parent.is_empty() && !directories.contains(parent) {
                    "409 Conflict"
                } else {
                    directories.insert(path.clone());
                    "201 Created"
                };
                paths.push(path);
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            }
            paths
        });
        let client = WebDavClient::new(
            WebDavSource {
                endpoint: format!("http://{address}"),
                username: String::new(),
                password: String::new(),
                remote_path: "/Chronicle".into(),
            },
            RequestPolicy {
                request_delay_ms: 0,
                retry_limit: 0,
                ..RequestPolicy::default()
            },
        )
        .unwrap();
        let first = client.ensure_collection("sync-v2/operations").await;
        let second = if first.is_ok() {
            client.ensure_collection("sync-v2/operations/").await
        } else {
            Ok(())
        };
        let paths = server.join().unwrap();
        assert!(
            first.is_ok(),
            "nested MKCOL failed: {first:?}; requests: {paths:?}"
        );
        assert!(second.is_ok());
        assert_eq!(
            paths,
            [
                "/Chronicle",
                "/Chronicle/sync-v2",
                "/Chronicle/sync-v2/operations",
                "/Chronicle",
                "/Chronicle/sync-v2",
                "/Chronicle/sync-v2/operations"
            ]
        );
    }
    #[test]
    fn denied_or_malformed_directory_listing_is_not_an_empty_success() {
        let denied = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/Chronicle/sync-v2/operations/</d:href><d:status>HTTP/1.1 403 Forbidden</d:status></d:response></d:multistatus>"#;
        assert!(
            super::parse_json_listing(
                denied,
                "https://dav.example.test/Chronicle/sync-v2/operations/",
                "sync-v2/operations/"
            )
            .is_err()
        );
        assert!(
            super::parse_json_listing(
                "not xml",
                "https://dav.example.test/Chronicle/sync-v2/operations/",
                "sync-v2/operations/"
            )
            .is_err()
        );
    }
    #[test]
    fn namespaced_listing_decodes_paths_and_rejects_foreign_roots() {
        let xml = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/Chronicle/sync-v2/operations/%61.json</d:href><d:status>HTTP/1.1 200 OK</d:status></d:response></d:multistatus>"#;
        assert_eq!(
            super::parse_json_listing(
                xml,
                "https://dav.example.test/Chronicle/sync-v2/operations/",
                "sync-v2/operations/"
            )
            .unwrap(),
            vec!["sync-v2/operations/a.json"]
        );
        assert!(
            super::parse_json_listing(
                &xml.replace("/Chronicle/sync-v2/operations/%61.json", "/Other/a.json"),
                "https://dav.example.test/Chronicle/sync-v2/operations/",
                "sync-v2/operations/"
            )
            .is_err()
        );
    }

    #[test]
    fn remote_urls_stay_below_the_configured_chronicle_root() {
        let client = WebDavClient::new(
            WebDavSource {
                endpoint: "https://dav.example.test/user/".into(),
                username: "user".into(),
                password: "secret".into(),
                remote_path: "/Chronicle/".into(),
            },
            RequestPolicy::default(),
        )
        .unwrap();
        assert_eq!(
            client.url("data/catalog.json"),
            "https://dav.example.test/user/Chronicle/data/catalog.json"
        );
        assert_eq!(client.url(""), "https://dav.example.test/user/Chronicle");
    }

    #[test]
    fn move_destination_urls_percent_encode_archive_names() {
        let client = WebDavClient::new(
            WebDavSource {
                endpoint: "https://dav.example.test/user/".into(),
                username: "user".into(),
                password: "secret".into(),
                remote_path: "/Chronicle/".into(),
            },
            RequestPolicy::default(),
        )
        .unwrap();

        assert_eq!(
            client.url("archives/新建文本文档.txt"),
            "https://dav.example.test/user/Chronicle/archives/%E6%96%B0%E5%BB%BA%E6%96%87%E6%9C%AC%E6%96%87%E6%A1%A3.txt"
        );
    }

    #[test]
    fn only_jianguoyun_uses_the_non_atomic_overwrite_fallback() {
        assert!(requires_delete_before_overwrite(
            "https://dav.jianguoyun.com/dav/"
        ));
        assert!(!requires_delete_before_overwrite(
            "https://dav.example.test/user/"
        ));
    }
}

#[derive(Debug, Error)]
pub enum WebDavError {
    #[error("{0}")]
    Remote(String),
    #[error("WebDAV request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("WebDAV returned HTTP {status} for {operation}")]
    Status { status: u16, operation: String },
    #[error("remote object was not found: {0}")]
    NotFound(String),
    #[error("local I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("remote JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid WebDAV method: {0}")]
    InvalidMethod(String),
}

pub type Result<T> = std::result::Result<T, WebDavError>;

fn requires_delete_before_overwrite(endpoint: &str) -> bool {
    let endpoint = endpoint.trim_start().to_ascii_lowercase();
    endpoint.starts_with("https://dav.jianguoyun.com/")
        || endpoint.starts_with("http://dav.jianguoyun.com/")
}

/// Small `WebDAV` client with bounded retries and atomic temporary-object uploads.
#[derive(Clone)]
pub struct WebDavClient {
    client: Client,
    source: WebDavSource,
    policy: RequestPolicy,
}

impl WebDavClient {
    pub fn new(source: WebDavSource, policy: RequestPolicy) -> Result<Self> {
        Ok(Self {
            client: Client::builder().timeout(Duration::from_mins(1)).build()?,
            source,
            policy,
        })
    }

    #[must_use]
    pub fn url(&self, relative: &str) -> String {
        let endpoint = self.source.endpoint.trim_end_matches('/');
        let root = self.source.remote_path.trim_matches('/');
        let relative = relative.trim_start_matches('/');
        let url = match (root.is_empty(), relative.is_empty()) {
            (true, true) => endpoint.to_owned(),
            (true, false) => format!("{endpoint}/{relative}"),
            (false, true) => format!("{endpoint}/{root}"),
            (false, false) => format!("{endpoint}/{root}/{relative}"),
        };
        reqwest::Url::parse(&url).map_or(url, |parsed| parsed.to_string())
    }

    async fn request(
        &self,
        method: Method,
        relative: &str,
        body: Option<Vec<u8>>,
        destination: Option<String>,
    ) -> Result<reqwest::Response> {
        let url = self.url(relative);
        for attempt in 0..=self.policy.retry_limit {
            if self.policy.request_delay_ms > 0 {
                sleep(Duration::from_millis(self.policy.request_delay_ms)).await;
            }
            let mut request = self
                .client
                .request(method.clone(), &url)
                .basic_auth(&self.source.username, Some(&self.source.password));
            if let Some(value) = destination.as_ref() {
                request = request
                    .header("Destination", value)
                    .header("Overwrite", "T");
            }
            if method.as_str() == "PROPFIND" {
                request = request.header("Depth", "1");
            }
            if let Some(bytes) = body.as_ref() {
                request = request.body(bytes.clone());
            }
            let response = request.send().await?;
            let status = response.status();
            if status.is_success()
                || status == StatusCode::MULTI_STATUS
                || (method.as_str() == "MKCOL" && status == StatusCode::METHOD_NOT_ALLOWED)
            {
                return Ok(response);
            }
            if status == StatusCode::NOT_FOUND {
                return Err(WebDavError::NotFound(relative.into()));
            }
            let retry_after_ms = response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(|seconds| seconds.saturating_mul(1_000));
            if let Some(delay) = self.policy.retry_delay_ms(
                status.as_u16(),
                attempt,
                retry_after_ms,
                u64::from(attempt) * 7919,
            ) {
                sleep(Duration::from_millis(delay)).await;
                continue;
            }
            return Err(WebDavError::Status {
                status: status.as_u16(),
                operation: format!("{} {relative}", method.as_str()),
            });
        }
        unreachable!("retry loop always returns")
    }

    pub async fn ensure_collection(&self, relative: &str) -> Result<()> {
        let method = Method::from_bytes(b"MKCOL")
            .map_err(|error| WebDavError::InvalidMethod(error.to_string()))?;
        self.request(method.clone(), "", None, None).await?;
        let mut prefix = String::new();
        for part in relative.split('/').filter(|part| !part.is_empty()) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            self.request(method.clone(), &prefix, None, None).await?;
        }
        Ok(())
    }

    pub async fn get_bytes(&self, relative: &str) -> Result<Vec<u8>> {
        Ok(self
            .request(Method::GET, relative, None, None)
            .await?
            .bytes()
            .await?
            .to_vec())
    }

    pub async fn get_json<T: DeserializeOwned>(&self, relative: &str) -> Result<T> {
        Ok(serde_json::from_slice(&self.get_bytes(relative).await?)?)
    }

    pub async fn put_atomic(&self, relative: &str, body: Vec<u8>) -> Result<()> {
        let temporary = format!("{relative}.chronicle-upload-{}", Uuid::new_v4());
        self.request(Method::PUT, &temporary, Some(body), None)
            .await?;
        let destination = self.url(relative);
        let move_method = Method::from_bytes(b"MOVE")
            .map_err(|error| WebDavError::InvalidMethod(error.to_string()))?;
        let move_result = self
            .request(move_method, &temporary, None, Some(destination))
            .await
            .map(|_| ());
        let move_result = match move_result {
            Err(WebDavError::Status { status: 409, .. })
                if requires_delete_before_overwrite(&self.source.endpoint) =>
            {
                // Jianguoyun accepts MOVE but rejects an overwrite MOVE with 409. The temporary
                // object is already complete, so only this provider takes the delete-and-retry path.
                self.delete(relative).await?;
                let move_method = Method::from_bytes(b"MOVE")
                    .map_err(|error| WebDavError::InvalidMethod(error.to_string()))?;
                self.request(move_method, &temporary, None, Some(self.url(relative)))
                    .await
                    .map(|_| ())
            }
            result => result,
        };
        if let Err(error) = move_result {
            let _ = self.delete(&temporary).await;
            return Err(error);
        }
        Ok(())
    }

    pub async fn put_json<T: Serialize>(&self, relative: &str, value: &T) -> Result<()> {
        self.put_atomic(relative, serde_json::to_vec_pretty(value)?)
            .await
    }

    pub async fn upload_file(&self, relative: &str, path: &Path) -> Result<()> {
        self.put_atomic(relative, tokio::fs::read(path).await?)
            .await
    }

    pub async fn download_file(&self, relative: &str, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, self.get_bytes(relative).await?).await?;
        Ok(())
    }

    pub async fn delete(&self, relative: &str) -> Result<()> {
        match self.request(Method::DELETE, relative, None, None).await {
            Ok(_) | Err(WebDavError::NotFound(_)) => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub async fn move_object(&self, source: &str, destination: &str) -> Result<()> {
        let method = Method::from_bytes(b"MOVE")
            .map_err(|error| WebDavError::InvalidMethod(error.to_string()))?;
        self.request(method, source, None, Some(self.url(destination)))
            .await
            .map(|_| ())
    }

    pub async fn test_capabilities(&self) -> Result<()> {
        let propfind = Method::from_bytes(b"PROPFIND")
            .map_err(|error| WebDavError::InvalidMethod(error.to_string()))?;
        match self.request(propfind, "", None, None).await {
            Ok(_) => {}
            Err(WebDavError::NotFound(_)) => self.ensure_collection("").await?,
            Err(error) => return Err(error),
        }
        let probe = format!(".chronicle-probe-{}", Uuid::new_v4());
        let moved = format!("{probe}-moved");
        self.request(Method::PUT, &probe, Some(b"chronicle".to_vec()), None)
            .await?;
        let move_method = Method::from_bytes(b"MOVE")
            .map_err(|error| WebDavError::InvalidMethod(error.to_string()))?;
        self.request(move_method, &probe, None, Some(self.url(&moved)))
            .await?;
        self.put_atomic(&moved, b"chronicle-updated".to_vec())
            .await?;
        self.delete(&moved).await
    }

    /// Lists direct JSON children only; malformed or out-of-root responses fail closed.
    pub async fn list_json_files(&self, relative: &str) -> Result<Vec<String>> {
        crate::validate_relative_path(relative)?;
        let method = Method::from_bytes(b"PROPFIND")
            .map_err(|error| WebDavError::InvalidMethod(error.to_string()))?;
        let response = self.request(method, relative, None, None).await?;
        parse_json_listing(&response.text().await?, &self.url(relative), relative)
    }
}

fn parse_json_listing(xml: &str, base: &str, relative: &str) -> Result<Vec<String>> {
    use quick_xml::{Reader, events::Event};
    let base = reqwest::Url::parse(&format!("{}/", base.trim_end_matches('/')))
        .map_err(|error| WebDavError::Remote(error.to_string()))?;
    let mut reader = Reader::from_str(xml);
    let mut files = Vec::new();
    let mut root_seen = false;
    let mut status_seen = false;
    loop {
        match reader
            .read_event()
            .map_err(|error| WebDavError::Remote(error.to_string()))?
        {
            Event::Start(event) if event.local_name().as_ref() == b"multistatus" => {
                root_seen = true;
            }
            Event::Start(event) if event.local_name().as_ref() == b"status" => {
                let text = reader
                    .read_text(event.name())
                    .map_err(|error| WebDavError::Remote(error.to_string()))?;
                let text = text
                    .decode()
                    .map_err(|error| WebDavError::Remote(error.to_string()))?;
                let status = text
                    .split_whitespace()
                    .nth(1)
                    .ok_or_else(|| WebDavError::Remote("目录列举状态无效".into()))?;
                if !matches!(status, "200" | "207") {
                    return Err(WebDavError::Remote(format!(
                        "目录列举返回读取错误：{status}"
                    )));
                }
                status_seen = true;
            }
            Event::Start(event) if event.local_name().as_ref() == b"href" => {
                let text = reader
                    .read_text(event.name())
                    .map_err(|error| WebDavError::Remote(error.to_string()))?;
                let text = text
                    .decode()
                    .map_err(|error| WebDavError::Remote(error.to_string()))?;
                let text = quick_xml::escape::unescape(&text)
                    .map_err(|error| WebDavError::Remote(error.to_string()))?;
                let url = base
                    .join(&text)
                    .map_err(|error| WebDavError::Remote(error.to_string()))?;
                if url.origin() != base.origin() {
                    return Err(WebDavError::Remote("远端列举包含越界地址".into()));
                }
                if url.path().trim_end_matches('/') == base.path().trim_end_matches('/') {
                    continue;
                }
                let suffix = url
                    .path()
                    .strip_prefix(base.path())
                    .ok_or_else(|| WebDavError::Remote("远端列举包含越界路径".into()))?;
                let suffix = urlencoding::decode(suffix)
                    .map_err(|error| WebDavError::Remote(error.to_string()))?;
                if suffix.ends_with('/') {
                    continue;
                }
                crate::validate_relative_path(&suffix)?;
                if suffix.contains('/') {
                    return Err(WebDavError::Remote("远端列举层级不正确".into()));
                }
                if suffix.ends_with(".json") {
                    files.push(format!("{}/{suffix}", relative.trim_end_matches('/')));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !root_seen || !status_seen {
        return Err(WebDavError::Remote("远端列举响应无效".into()));
    }
    files.sort();
    files.dedup();
    Ok(files)
}
