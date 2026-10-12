#![doc = "Repository comparison and rate-limited synchronization orchestration."]

mod github;
mod remote;
pub mod snapshot_protocol;
mod webdav;
pub use remote::{
    ImmutableCache, OPEN_DAL_SCHEMES, OpenDalSource, PUBLIC_CONFIG_KEYS, RemoteStore,
    validate_relative_path,
};

pub use github::{
    CreatedGitHubRepository, GitHubChange, GitHubClient, GitHubError, GitHubSource,
    MAX_GITHUB_FILE_BYTES,
};
pub use webdav::{WebDavClient, WebDavError, WebDavSource};

/// Conservative request limits shared by every remote adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestPolicy {
    /// Maximum simultaneous catalog or timeline reads.
    pub max_concurrent_metadata_reads: usize,
    /// Maximum combined simultaneous uploads and downloads.
    pub max_concurrent_transfers: usize,
    /// Minimum spacing between starting remote requests.
    pub request_delay_ms: u64,
    /// Maximum retries for one temporary failure.
    pub retry_limit: u8,
    /// Initial exponential-backoff delay.
    pub base_retry_delay_ms: u64,
    /// Maximum client-generated exponential-backoff delay.
    pub max_retry_delay_ms: u64,
}

impl Default for RequestPolicy {
    fn default() -> Self {
        Self {
            max_concurrent_metadata_reads: 2,
            max_concurrent_transfers: 2,
            request_delay_ms: 150,
            retry_limit: 5,
            base_retry_delay_ms: 1_000,
            max_retry_delay_ms: 30_000,
        }
    }
}

impl RequestPolicy {
    /// Computes the next delay for a temporary HTTP failure.
    ///
    /// `attempt` starts at zero. A parsed `Retry-After` value is always honored
    /// when it exceeds the client-generated delay.
    #[must_use]
    pub fn retry_delay_ms(
        &self,
        status: u16,
        attempt: u8,
        retry_after_ms: Option<u64>,
        jitter_seed: u64,
    ) -> Option<u64> {
        if attempt >= self.retry_limit || !is_temporary_status(status) {
            return None;
        }
        let multiplier = 1_u64.checked_shl(u32::from(attempt)).unwrap_or(u64::MAX);
        let exponential = self
            .base_retry_delay_ms
            .saturating_mul(multiplier)
            .min(self.max_retry_delay_ms);
        let jitter = jitter_seed % (exponential / 4 + 1);
        Some(
            exponential
                .saturating_add(jitter)
                .max(retry_after_ms.unwrap_or_default()),
        )
    }
}

#[must_use]
fn is_temporary_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

#[cfg(test)]
mod tests {
    use super::RequestPolicy;

    #[test]
    fn temporary_failures_back_off_and_honor_server_delay() {
        let policy = RequestPolicy::default();
        assert_eq!(policy.retry_delay_ms(503, 0, Some(8_000), 0), Some(8_000));
        assert_eq!(policy.retry_delay_ms(503, 1, None, 0), Some(2_000));
        assert_eq!(policy.retry_delay_ms(404, 0, None, 0), None);
        assert_eq!(policy.retry_delay_ms(503, 5, None, 0), None);
    }
}
