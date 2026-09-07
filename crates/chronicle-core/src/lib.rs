#![doc = "Domain types and application boundaries for Chronicle."]

use serde::{Deserialize, Serialize};

/// The kind of source tracked by Chronicle.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// A single file.
    File,
    /// A directory tree.
    Directory,
}

/// A locally configured file or directory.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Entry {
    /// Stable identifier independent of the source path.
    pub id: String,
    /// User-facing name.
    pub name: String,
    /// Source path on this device.
    pub source_path: String,
    /// Source type.
    pub kind: EntryKind,
    /// Optional local category identifier.
    pub category_id: Option<String>,
    /// Time the entry was registered, expressed as Unix milliseconds.
    pub created_at_ms: u64,
}

/// Metadata for one file contained in a snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotFile {
    /// Forward-slash-separated path relative to the entry root.
    pub relative_path: String,
    /// File content hash.
    pub content_hash: String,
    /// Uncompressed byte count.
    pub size_bytes: u64,
    /// Source modification time expressed as Unix milliseconds.
    pub modified_at_ms: u64,
}

/// File-level difference from a snapshot's direct parent.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChangeSummary {
    /// Files absent from the parent snapshot.
    pub added: u64,
    /// Files whose content differs from the parent snapshot.
    pub modified: u64,
    /// Files removed since the parent snapshot.
    pub deleted: u64,
}

/// Immutable metadata for one point-in-time backup.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Snapshot {
    /// Globally unique snapshot identifier.
    pub id: String,
    /// Entry captured by the snapshot.
    pub entry_id: String,
    /// Optional direct parent snapshot.
    pub parent_id: Option<String>,
    /// Device that created the snapshot.
    pub device_id: String,
    /// User-facing snapshot name.
    pub title: String,
    /// Creation time expressed as Unix milliseconds.
    pub created_at_ms: u64,
    /// Hash of the immutable stored object.
    pub object_hash: String,
    /// Stored object size in bytes.
    pub size_bytes: u64,
    /// Ordered file manifest used for comparison and verification.
    pub files: Vec<SnapshotFile>,
    /// Difference from the direct parent.
    pub changes: ChangeSummary,
    /// Whether this was created automatically before a restore.
    pub safety: bool,
}
