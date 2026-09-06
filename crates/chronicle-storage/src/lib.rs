#![doc = "Local metadata and immutable object storage adapters."]

use chronicle_core::Snapshot;

/// Describes an object that has been committed to a repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredObject {
    /// Content hash used as the object key.
    pub hash: String,
    /// Stored byte count.
    pub size_bytes: u64,
}

/// Builds a stable object key for a snapshot.
#[must_use]
pub fn object_key(snapshot: &Snapshot) -> String {
    let prefix = snapshot.object_hash.get(..2).unwrap_or("00");
    format!("objects/{prefix}/{}", snapshot.object_hash)
}

#[cfg(test)]
mod tests {
    use chronicle_core::Snapshot;

    use super::object_key;

    #[test]
    fn object_key_uses_hash_prefix() {
        let snapshot = Snapshot {
            id: "snapshot-1".into(),
            entry_id: "entry-1".into(),
            parent_id: None,
            device_id: "device-1".into(),
            created_at_ms: 0,
            object_hash: "abcdef".into(),
            size_bytes: 6,
        };

        assert_eq!(object_key(&snapshot), "objects/ab/abcdef");
    }
}
