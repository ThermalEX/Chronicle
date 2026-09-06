#![doc = "Domain types and application boundaries for Chronicle."]

/// The kind of source tracked by Chronicle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    /// A single file.
    File,
    /// A directory tree.
    Directory,
}

/// A locally configured file or directory.
#[derive(Clone, Debug, Eq, PartialEq)]
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
}

/// Immutable metadata for one point-in-time backup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    /// Globally unique snapshot identifier.
    pub id: String,
    /// Entry captured by the snapshot.
    pub entry_id: String,
    /// Optional direct parent snapshot.
    pub parent_id: Option<String>,
    /// Device that created the snapshot.
    pub device_id: String,
    /// Creation time expressed as Unix milliseconds.
    pub created_at_ms: u64,
    /// Hash of the immutable stored object.
    pub object_hash: String,
    /// Stored object size in bytes.
    pub size_bytes: u64,
}
