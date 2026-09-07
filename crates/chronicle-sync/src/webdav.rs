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
    use super::{WebDavClient, WebDavSource};
    use crate::RequestPolicy;

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
}

#[derive(Debug, Error)]
pub enum WebDavError {
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
        match (root.is_empty(), relative.is_empty()) {
            (true, true) => endpoint.to_owned(),
            (true, false) => format!("{endpoint}/{relative}"),
            (false, true) => format!("{endpoint}/{root}"),
            (false, false) => format!("{endpoint}/{root}/{relative}"),
        }
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
                request = request.header("Depth", "0");
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
        self.request(method, relative, None, None).await.map(|_| ())
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
        if let Err(error) = self
            .request(move_method, &temporary, None, Some(destination))
            .await
        {
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
        self.delete(&moved).await
    }
}
