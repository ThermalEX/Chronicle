#![doc = "Repository comparison and resumable synchronization orchestration."]

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

/// One immutable object transfer selected by the comparison phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transfer {
    /// Content-addressed object key.
    pub object_key: String,
    /// Expected size used for progress and verification.
    pub size_bytes: u64,
}
