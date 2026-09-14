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

fn deletion_tree_entry(path: &str) -> Value {
    json!({ "path": path, "mode": "100644", "type": "blob", "sha": Value::Null })
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
    #[error("GitHub repository name is invalid")]
    InvalidNewRepositoryName,
    #[error("GitHub remote path is invalid")]
    InvalidPath,
    #[error("GitHub single-file limit is 100 MB: {path}")]
    FileTooLarge { path: String },
}

pub type Result<T> = std::result::Result<T, GitHubError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedGitHubRepository {
    pub repository: String,
    pub branch: String,
}

fn valid_new_repository_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 100
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return Err(GitHubError::InvalidNewRepositoryName);
    }
    Ok(name.to_owned())
}

#[derive(Clone)]
pub struct GitHubClient {
    client: Client,
    api_root: String,
    source: GitHubSource,
    policy: RequestPolicy,
}

impl GitHubClient {
    /// Creates an initialized private repository for the authenticated GitHub account.
    ///
    /// An initial commit makes the returned default branch immediately usable by the Git Data API.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested name is unsafe, the GitHub request fails, or GitHub
    /// rejects the authenticated account's repository creation request.
    pub async fn create_private_repository(
        token: &str,
        repository_name: &str,
        policy: RequestPolicy,
    ) -> Result<CreatedGitHubRepository> {
        let repository_name = valid_new_repository_name(repository_name)?;
        let client = Client::builder().timeout(Duration::from_mins(1)).build()?;
        for attempt in 0..=policy.retry_limit {
            if policy.request_delay_ms > 0 {
                sleep(Duration::from_millis(policy.request_delay_ms)).await;
            }
            let response = client
                .post("https://api.github.com/user/repos")
                .header(header::ACCEPT, "application/vnd.github+json")
                .header(header::USER_AGENT, "Chronicle")
                .bearer_auth(token)
                .header("X-GitHub-Api-Version", "2022-11-28")
                .json(&json!({
                    "name": repository_name,
                    "description": "Chronicle backup library",
                    "private": true,
                    "auto_init": true,
                }))
                .send()
                .await?;
            let status = response.status();
            if status.is_success() {
                let value: Value = response.json().await?;
                let repository = value
                    .get("full_name")
                    .and_then(Value::as_str)
                    .filter(|name| name.split('/').filter(|part| !part.is_empty()).count() == 2)
                    .ok_or(GitHubError::InvalidRepository)?
                    .to_owned();
                let branch = value
                    .get("default_branch")
                    .and_then(Value::as_str)
                    .filter(|branch| !branch.trim().is_empty())
                    .ok_or(GitHubError::InvalidPath)?
                    .to_owned();
                return Ok(CreatedGitHubRepository { repository, branch });
            }
            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                && attempt < policy.retry_limit
            {
                sleep(Duration::from_millis(200 * 2_u64.pow(attempt.into()))).await;
                continue;
            }
            let message = response.text().await.unwrap_or_default();
            return Err(GitHubError::Status {
                status: status.as_u16(),
                operation: "POST /user/repos".into(),
                message,
            });
        }
        unreachable!("retry loop always returns")
    }

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
            api_root: format!("https://api.github.com/repos/{}", source.repository),
            source,
            policy,
        })
    }

    fn api(&self, suffix: &str) -> String {
        let root = &self.api_root;
        let suffix = suffix.trim_start_matches('/');
        if suffix.is_empty() {
            root.clone()
        } else {
            format!("{root}/{suffix}")
        }
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
            .request(reqwest::Method::GET, format!("branches/{branch}"), None)
            .await?
            .json()
            .await?;
        let head = reference
            .pointer("/commit/sha")
            .and_then(Value::as_str)
            .ok_or(GitHubError::InvalidPath)?
            .to_owned();
        let tree = reference
            .pointer("/commit/commit/tree/sha")
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
                // Inline small JSON metadata in the tree; archives still use lossless base64 blobs.
                if std::path::Path::new(&path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                    && contents.len() <= 1024 * 1024
                    && let Ok(content) = std::str::from_utf8(&contents)
                {
                    tree.push(json!({ "path": path, "mode": "100644", "type": "blob", "content": content }));
                    continue;
                }
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
                deletion_tree_entry(&path)
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
        if tree_sha == base_tree {
            return Ok(());
        }
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
        .await?
        .bytes()
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GitHubChange, GitHubClient, GitHubSource, deletion_tree_entry, valid_new_repository_name,
    };
    use crate::RequestPolicy;
    use serde_json::{Value, json};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};

    type RecordedRequests = Arc<Mutex<Vec<(String, Value)>>>;

    // Only HTTP is faked: exercise the real request/commit pipeline and inspect its payloads.
    fn git_server(unchanged: bool, conflict: bool) -> (GitHubClient, RecordedRequests) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::downgrade(&requests);
        std::thread::spawn(move || {
            while let Some(recorded) = recorded.upgrade() {
                let Ok((mut stream, _)) = listener.accept() else {
                    drop(recorded);
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    continue;
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let route = line
                    .split_whitespace()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut length = 0;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
                recorded.lock().unwrap().push((route.clone(), body));
                let response = match route.as_str() {
                    "GET /branches/main" => {
                        json!({"commit": {"sha": "head", "commit": {"tree": {"sha": "base"}}}})
                    }
                    "GET /git/ref/heads/main" => json!({"object": {"sha": "head"}}),
                    "GET /git/commits/head" => json!({"tree": {"sha": "base"}}),
                    "POST /git/blobs" => json!({"sha": "blob"}),
                    "POST /git/trees" => json!({"sha": if unchanged { "base" } else { "tree" }}),
                    "POST /git/commits" => json!({"sha": "commit"}),
                    "PATCH /git/refs/heads/main" => json!({"object": {"sha": "commit"}}),
                    _ => panic!("unexpected request: {route}"),
                }
                .to_string();
                let status = if conflict && route.starts_with("PATCH") {
                    "422 Unprocessable Entity"
                } else {
                    "200 OK"
                };
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
            }
        });
        let mut client = GitHubClient::new(
            GitHubSource {
                repository: "owner/repository".into(),
                branch: "main".into(),
                remote_path: "/Chronicle".into(),
                token: "test-only".into(),
            },
            RequestPolicy {
                request_delay_ms: 0,
                retry_limit: 0,
                ..RequestPolicy::default()
            },
        )
        .unwrap();
        client.api_root = format!("http://{address}");
        client.client = reqwest::Client::builder().no_proxy().build().unwrap();
        (client, requests)
    }

    #[tokio::test]
    async fn deletion_and_catalog_update_need_only_four_requests() {
        let (client, requests) = git_server(false, false);
        let catalog = "{\"name\":\"中文存档\"}";
        client
            .commit_changes(
                "delete",
                vec![
                    GitHubChange {
                        path: "archives/a/old.7z".into(),
                        contents: None,
                    },
                    GitHubChange {
                        path: "catalog.json".into(),
                        contents: Some(catalog.as_bytes().to_vec()),
                    },
                ],
            )
            .await
            .unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(
            requests.len(),
            4,
            "metadata must not cost an extra blob upload or head lookup"
        );
        let tree = &requests
            .iter()
            .find(|(route, _)| route == "POST /git/trees")
            .unwrap()
            .1;
        assert_eq!(tree["base_tree"], "base");
        assert_eq!(
            tree["tree"][0],
            json!({"path":"Chronicle/archives/a/old.7z","mode":"100644","type":"blob","sha":null})
        );
        assert_eq!(tree["tree"][1]["content"], catalog);
        assert!(tree["tree"][1].get("sha").is_none());
        assert_eq!(
            requests.last().unwrap().1,
            json!({"sha":"commit","force":false})
        );
    }

    #[tokio::test]
    async fn binary_snapshot_upload_keeps_exact_bytes() {
        use base64::Engine;
        let (client, requests) = git_server(false, false);
        client
            .commit_changes(
                "upload",
                vec![GitHubChange {
                    path: "archives/a/new.7z".into(),
                    contents: Some(vec![0, 255, 128, 42]),
                }],
            )
            .await
            .unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 5);
        let blob = &requests
            .iter()
            .find(|(route, _)| route == "POST /git/blobs")
            .unwrap()
            .1;
        assert_eq!(blob["encoding"], "base64");
        assert_eq!(
            super::STANDARD
                .decode(blob["content"].as_str().unwrap())
                .unwrap(),
            [0, 255, 128, 42]
        );
    }

    #[tokio::test]
    async fn unchanged_tree_does_not_create_a_commit_or_update_the_branch() {
        let (client, requests) = git_server(true, false);
        client
            .commit_changes(
                "unchanged",
                vec![GitHubChange {
                    path: "category-tree.json".into(),
                    contents: Some(b"{}".to_vec()),
                }],
            )
            .await
            .unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            !requests
                .iter()
                .any(|(route, _)| route.starts_with("PATCH") || route == "POST /git/commits")
        );
    }

    #[tokio::test]
    async fn concurrent_branch_change_is_reported_without_force_push() {
        let (client, requests) = git_server(false, true);
        assert!(
            client
                .commit_changes(
                    "update",
                    vec![GitHubChange {
                        path: "catalog.json".into(),
                        contents: Some(b"{}".to_vec()),
                    }]
                )
                .await
                .is_err()
        );
        let requests = requests.lock().unwrap();
        let patches: Vec<_> = requests
            .iter()
            .filter(|(route, _)| route.starts_with("PATCH"))
            .collect();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].1["force"], false);
    }

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
    fn deletion_tree_entries_keep_the_required_blob_mode_and_type() {
        let entry = deletion_tree_entry("Chronicle/archives/archive/snapshot.7z");

        assert_eq!(entry["path"], "Chronicle/archives/archive/snapshot.7z");
        assert_eq!(entry["mode"], "100644");
        assert_eq!(entry["type"], "blob");
        assert!(entry["sha"].is_null());
    }

    #[test]
    fn repository_endpoint_has_no_trailing_slash() {
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
            client.api(""),
            "https://api.github.com/repos/owner/repository"
        );
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

    #[test]
    fn accepts_a_safe_new_repository_name() {
        assert!(valid_new_repository_name("chronicle").is_ok());
        assert!(valid_new_repository_name("Chronicle Backups").is_ok());
        assert!(valid_new_repository_name("../outside").is_err());
        assert!(valid_new_repository_name("").is_err());
    }
}
