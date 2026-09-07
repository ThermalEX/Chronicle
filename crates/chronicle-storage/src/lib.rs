#![doc = "Local metadata and immutable object storage adapters."]

mod repository;

use chronicle_core::Snapshot;

pub use repository::{LocalRepository, Result, StorageError};

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
