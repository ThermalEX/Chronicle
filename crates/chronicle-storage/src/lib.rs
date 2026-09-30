#![doc = "Local metadata and immutable object storage adapters."]

pub mod exclusions;
pub mod health;
pub mod registry;
mod repository;

use chronicle_core::Snapshot;

pub use repository::{DeviceIdentity, LocalRepository, Result, SnapshotDeletion, StorageError};

/// Automatic capture can safely skip or abandon work without creating a snapshot.
#[derive(Debug)]
// Returned one at a time; retain the owned snapshot without a separate allocation.
#[allow(clippy::large_enum_variant)]
pub enum AutomaticSnapshotOutcome {
    Created(Snapshot),
    Unchanged,
    Superseded,
}

/// Describes an object that has been committed to a repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredObject {
    /// Content hash used as the object key.
    pub hash: String,
    /// Stored byte count.
    pub size_bytes: u64,
}

/// Builds the relative path for a snapshot archive.
#[must_use]
pub fn object_key(snapshot: &Snapshot) -> String {
    snapshot.archive_name.clone()
}
