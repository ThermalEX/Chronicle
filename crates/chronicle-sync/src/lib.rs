#![doc = "Repository comparison and rate-limited synchronization orchestration."]

mod github;
mod webdav;

pub use github::{
    CreatedGitHubRepository, GitHubChange, GitHubClient, GitHubError, GitHubSource,
    MAX_GITHUB_FILE_BYTES,
};
pub use webdav::{WebDavClient, WebDavError, WebDavSource};

/// How a synchronization job was started.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncTrigger {
    /// The user explicitly requested synchronization.
    Manual,
    /// A configured schedule started synchronization.
    Scheduled,
    /// A local file change started synchronization.
    FileChanged,
}

/// Direction of one immutable archive transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferDirection {
    /// Publish a local archive to the remote repository.
    Upload,
    /// Materialize a remote archive in the local repository.
    Download,
}

/// One immutable object transfer selected by the comparison phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transfer {
    /// Repository-relative archive path.
    pub object_key: String,
    /// Expected size used for progress and verification.
    pub size_bytes: u64,
    /// Transfer direction.
    pub direction: TransferDirection,
}

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
    /// Returns metadata reads in waves that never exceed the configured concurrency.
    #[must_use]
    pub fn metadata_batches<'a, T>(&self, items: &'a [T]) -> Vec<&'a [T]> {
        items
            .chunks(self.max_concurrent_metadata_reads.max(1))
            .collect()
    }

    /// Returns uploads and downloads in one bounded queue.
    #[must_use]
    pub fn transfer_batches<'a>(&self, transfers: &'a [Transfer]) -> Vec<&'a [Transfer]> {
        transfers
            .chunks(self.max_concurrent_transfers.max(1))
            .collect()
    }

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

/// Whether a new synchronization request should start or join pending work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriggerDecision {
    /// No job is running, so the caller should start one.
    Start,
    /// A job is already running and one follow-up pass has been recorded.
    Coalesced,
}

/// Coalesces repeated events and button presses into at most one follow-up pass.
#[derive(Debug, Default)]
pub struct TriggerGate {
    running: bool,
    pending: bool,
}

impl TriggerGate {
    /// Registers a synchronization request.
    pub fn request(&mut self) -> TriggerDecision {
        if self.running {
            self.pending = true;
            TriggerDecision::Coalesced
        } else {
            self.running = true;
            TriggerDecision::Start
        }
    }

    /// Completes the current pass and reports whether one consolidated pass remains.
    pub fn finish_pass(&mut self) -> bool {
        if self.pending {
            self.pending = false;
            true
        } else {
            self.running = false;
            false
        }
    }
}

#[must_use]
fn is_temporary_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

#[cfg(test)]
mod tests {
    use super::{RequestPolicy, Transfer, TransferDirection, TriggerDecision, TriggerGate};

    #[test]
    fn default_policy_bounds_all_remote_work() {
        let policy = RequestPolicy::default();
        let metadata = [1, 2, 3, 4, 5];
        assert!(
            policy
                .metadata_batches(&metadata)
                .iter()
                .all(|batch| batch.len() <= 2)
        );

        let transfers = (0..5)
            .map(|index| Transfer {
                object_key: format!("{index}.7z"),
                size_bytes: 1,
                direction: if index % 2 == 0 {
                    TransferDirection::Upload
                } else {
                    TransferDirection::Download
                },
            })
            .collect::<Vec<_>>();
        assert!(
            policy
                .transfer_batches(&transfers)
                .iter()
                .all(|batch| batch.len() <= 2)
        );
    }

    #[test]
    fn temporary_failures_back_off_and_honor_server_delay() {
        let policy = RequestPolicy::default();
        assert_eq!(policy.retry_delay_ms(503, 0, Some(8_000), 0), Some(8_000));
        assert_eq!(policy.retry_delay_ms(503, 1, None, 0), Some(2_000));
        assert_eq!(policy.retry_delay_ms(404, 0, None, 0), None);
        assert_eq!(policy.retry_delay_ms(503, 5, None, 0), None);
    }

    #[test]
    fn repeated_triggers_create_only_one_follow_up_pass() {
        let mut gate = TriggerGate::default();
        assert_eq!(gate.request(), TriggerDecision::Start);
        assert_eq!(gate.request(), TriggerDecision::Coalesced);
        assert_eq!(gate.request(), TriggerDecision::Coalesced);
        assert!(gate.finish_pass());
        assert!(!gate.finish_pass());
    }
}
