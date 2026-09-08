use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{Client, StatusCode, header};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use thiserror::Error;
use tokio::time::sleep;

use crate::RequestPolicy;

pub const MAX_GITHUB_FILE_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct GitHubSource {
    pub repository: String,
    pub branch: String,
    pub remote_path: String,
    pub token: String,
}

#[derive(Clone, Debug)]
pub struct GitHubChange {
    pub path: String,
    pub contents: Option<Vec<u8>>,
}

#[derive(Debug, Error)]
pub enum GitHubError {
    #[error("GitHub request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("GitHub returned HTTP {status} for {operation}: {message}")]
    Status {
        status: u16,
        operation: String,
        message: String,
    },
    #[error("GitHub object was not found: {0}")]
    NotFound(String),
    #[error("GitHub response JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("GitHub repository must use owner/repository format")]
    InvalidRepository,
    #[error("GitHub remote path is invalid")]
    InvalidPath,
    #[error("GitHub single-file limit is 100 MB: {path}")]
    FileTooLarge { path: String },
}

pub type Result<T> = std::result::Result<T, GitHubError>;

#[derive(Clone)]
pub struct GitHubClient {
    client: Client,
    source: GitHubSource,
    policy: RequestPolicy,
}

impl GitHubClient {
    /// Creates a client after validating repository, branch, and root-directory input.
    ///
    /// # Errors
    ///
    /// Returns an error when the repository scope is invalid or the HTTP client cannot be built.
    pub fn new(source: GitHubSource, policy: RequestPolicy) -> Result<Self> {
        let valid_repo = source
            .repository
            .split('/')
            .filter(|part| !part.is_empty())
            .count()
            == 2;
        if !valid_repo {
            return Err(GitHubError::InvalidRepository);
        }
        if source.branch.trim().is_empty()
            || source.branch.split('/').any(|part| part == "..")
            || source
                .remote_path
                .trim_matches('/')
                .split('/')
                .any(|part| part == "..")
        {
            return Err(GitHubError::InvalidPath);
        }
        Ok(Self {
            client: Client::builder().timeout(Duration::from_mins(1)).build()?,
            source,
            policy,
        })
    }

    fn api(&self, suffix: &str) -> String {
        format!(
            "https://api.github.com/repos/{}/{}",
            self.source.repository,
            suffix.trim_start_matches('/')
        )
    }

    fn scoped_path(&self, path: &str) -> Result<String> {
        let path = path.trim_matches('/');
        if path
            .split('/')
            .any(|part| part == ".." || part.is_empty() && !path.is_empty())
        {
            return Err(GitHubError::InvalidPath);
        }
        let root = self.source.remote_path.trim_matches('/');
        Ok(if root.is_empty() {
            path.to_owned()
        } else if path.is_empty() {
            root.to_owned()
        } else {
            format!("{root}/{path}")
        })
    }

    async fn request(
        &self,
        method: reqwest::Method,
        suffix: String,
        body: Option<Value>,
    ) -> Result<reqwest::Response> {
        for attempt in 0..=self.policy.retry_limit {
            if self.policy.request_delay_ms > 0 {
                sleep(Duration::from_millis(self.policy.request_delay_ms)).await;
            }
            let mut request = self
                .client
                .request(method.clone(), self.api(&suffix))
                .header(header::ACCEPT, "application/vnd.github+json")
                .header(header::USER_AGENT, "Chronicle")
                .bearer_auth(&self.source.token)
                .header("X-GitHub-Api-Version", "2022-11-28");
            if let Some(body) = body.as_ref() {
                request = request.json(body);
            }
            let response = request.send().await?;
            let status = response.status();
            if status.is_success() {
                return Ok(response);
            }
            if status == StatusCode::NOT_FOUND {
                return Err(GitHubError::NotFound(suffix));
            }
            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                && attempt < self.policy.retry_limit
            {
                sleep(Duration::from_millis(200 * 2_u64.pow(attempt.into()))).await;
                continue;
            }
            let message = response.text().await.unwrap_or_default();
            return Err(GitHubError::Status {
                status: status.as_u16(),
                operation: suffix,
                message,
            });
        }
        unreachable!("retry loop always returns")
    }

    /// Verifies token, repository, and target-branch read access without writing to GitHub.
    ///
    /// # Errors
    ///
    /// Returns the GitHub API error that prevented verification.
    pub async fn test_access(&self) -> Result<()> {
        self.request(reqwest::Method::GET, String::new(), None)
            .await?;
        self.head().await.map(|_| ())
    }

    /// Reads and deserializes a JSON file from the configured Chronicle directory.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid scoped path, GitHub response, or JSON payload.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let path = self.scoped_path(path)?;
        let suffix = format!(
            "contents/{}?ref={}",
            path,
            urlencoding::encode(&self.source.branch)
        );
        let response = self.request(reqwest::Method::GET, suffix, None).await?;
        let value: Value = response.json().await?;
        let content = value
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| GitHubError::NotFound(path.clone()))?
            .replace('\n', "");
        serde_json::from_slice(
            &STANDARD
                .decode(content)
                .map_err(|error| GitHubError::Status {
                    status: 422,
                    operation: path,
                    message: error.to_string(),
                })?,
        )
        .map_err(GitHubError::Json)
    }

    /// Downloads one file from the configured Chronicle directory with retry throttling.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid path, an unavailable file, or an exhausted request retry.
    pub async fn download_file(&self, path: &str) -> Result<Vec<u8>> {
        let path = self.scoped_path(path)?;
        let suffix = format!(
            "contents/{}?ref={}",
            path,
            urlencoding::encode(&self.source.branch)
        );
        for attempt in 0..=self.policy.retry_limit {
            if self.policy.request_delay_ms > 0 {
                sleep(Duration::from_millis(self.policy.request_delay_ms)).await;
            }
            let response = self
                .client
                .get(self.api(&suffix))
                .header(header::ACCEPT, "application/vnd.github.raw+json")
                .header(header::USER_AGENT, "Chronicle")
                .bearer_auth(&self.source.token)
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await?;
            let status = response.status();
            if status.is_success() {
                return Ok(response.bytes().await?.to_vec());
            }
            if status == StatusCode::NOT_FOUND {
                return Err(GitHubError::NotFound(path));
            }
            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                && attempt < self.policy.retry_limit
            {
                sleep(Duration::from_millis(200 * 2_u64.pow(attempt.into()))).await;
                continue;
            }
            return Err(GitHubError::Status {
                status: status.as_u16(),
                operation: suffix,
                message: response.text().await.unwrap_or_default(),
            });
        }
        unreachable!("retry loop always returns")
    }

    async fn head(&self) -> Result<(String, String)> {
        let branch = urlencoding::encode(&self.source.branch);
        let reference: Value = self
            .request(
                reqwest::Method::GET,
                format!("git/ref/heads/{branch}"),
                None,
            )
            .await?
            .json()
            .await?;
        let head = reference
            .pointer("/object/sha")
            .and_then(Value::as_str)
            .ok_or(GitHubError::InvalidPath)?
            .to_owned();
        let commit: Value = self
            .request(reqwest::Method::GET, format!("git/commits/{head}"), None)
            .await?
            .json()
            .await?;
        let tree = commit
            .pointer("/tree/sha")
            .and_then(Value::as_str)
            .ok_or(GitHubError::InvalidPath)?
            .to_owned();
        Ok((head, tree))
    }

    /// Applies all supplied changes as one Git commit on the configured branch.
    ///
    /// # Errors
    ///
    /// Returns an error when a change escapes the Chronicle directory, exceeds 100 MB, or any
    /// Git Data API request fails.
    pub async fn commit_changes(&self, message: &str, changes: Vec<GitHubChange>) -> Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        let mut scoped_changes = Vec::with_capacity(changes.len());
        for change in changes {
            let path = self.scoped_path(&change.path)?;
            if change.contents.as_ref().is_some_and(|contents| {
                u64::try_from(contents.len()).unwrap_or(u64::MAX) > MAX_GITHUB_FILE_BYTES
            }) {
                return Err(GitHubError::FileTooLarge { path });
            }
            scoped_changes.push((path, change.contents));
        }
        let (head, base_tree) = self.head().await?;
        let mut tree = Vec::with_capacity(scoped_changes.len());
        for (path, contents) in scoped_changes {
            let entry = if let Some(contents) = contents {
                let blob: Value = self
                    .request(
                        reqwest::Method::POST,
                        "git/blobs".into(),
                        Some(json!({ "content": STANDARD.encode(contents), "encoding": "base64" })),
                    )
                    .await?
                    .json()
                    .await?;
                json!({ "path": path, "mode": "100644", "type": "blob", "sha": blob.get("sha") })
            } else {
                json!({ "path": path, "mode": Value::Null, "type": Value::Null, "sha": Value::Null })
            };
            tree.push(entry);
        }
        let tree_value: Value = self
            .request(
                reqwest::Method::POST,
                "git/trees".into(),
                Some(json!({ "base_tree": base_tree, "tree": tree })),
            )
            .await?
            .json()
            .await?;
        let tree_sha = tree_value
            .get("sha")
            .and_then(Value::as_str)
            .ok_or(GitHubError::InvalidPath)?;
        let commit: Value = self
            .request(
                reqwest::Method::POST,
                "git/commits".into(),
                Some(json!({ "message": message, "tree": tree_sha, "parents": [head] })),
            )
            .await?
            .json()
            .await?;
        let commit_sha = commit
            .get("sha")
            .and_then(Value::as_str)
            .ok_or(GitHubError::InvalidPath)?;
        let branch = urlencoding::encode(&self.source.branch);
        self.request(
            reqwest::Method::PATCH,
            format!("git/refs/heads/{branch}"),
            Some(json!({ "sha": commit_sha, "force": false })),
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{GitHubClient, GitHubSource};
    use crate::RequestPolicy;

    #[test]
    fn repository_path_stays_under_the_chronicle_root() {
        let client = GitHubClient::new(
            GitHubSource {
                repository: "owner/repository".into(),
                branch: "main".into(),
                remote_path: "/Chronicle".into(),
                token: "token".into(),
            },
            RequestPolicy::default(),
        )
        .unwrap();
        assert_eq!(
            client.scoped_path("catalog.json").unwrap(),
            "Chronicle/catalog.json"
        );
        assert!(client.scoped_path("../settings.json").is_err());
    }

    #[test]
    fn rejects_a_repository_root_that_escapes_the_repository() {
        let source = GitHubSource {
            repository: "owner/repository".into(),
            branch: "main".into(),
            remote_path: "../outside".into(),
            token: "token".into(),
        };
        assert!(GitHubClient::new(source, RequestPolicy::default()).is_err());
    }
}
