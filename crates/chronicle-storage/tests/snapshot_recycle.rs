use chronicle_storage::LocalRepository;
use std::fs;

fn fixture() -> (
    tempfile::TempDir,
    LocalRepository,
    String,
    chronicle_core::Snapshot,
) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("save.txt");
    fs::write(&source, "one").unwrap();
    let repo = LocalRepository::open(dir.path().join("repo")).unwrap();
    let entry = repo.add_entry(&source, None, None).unwrap();
    let snapshot = repo
        .create_snapshot(
            &entry.id,
            "first",
            &repo.device_identity().unwrap().0,
            false,
        )
        .unwrap();
    (dir, repo, entry.id, snapshot)
}

#[test]
fn deletion_recycles_and_restore_creates_a_new_id_without_revoking_the_delete() {
    let (_dir, repo, entry, old) = fixture();
    repo.delete_snapshot(&entry, &old.id).unwrap();
    assert!(repo.list_snapshots(&entry).unwrap().is_empty());
    let items = repo.list_snapshot_deletions().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].snapshot, old);
    assert_eq!(items[0].reason, "manual");
    let restored = repo
        .restore_recycled_snapshot(&items[0].operation_id)
        .unwrap();
    assert_ne!(restored.id, old.id);
    assert_eq!(restored.object_hash, old.object_hash);
    assert!(repo.verify_snapshot(&restored.id).unwrap());
    assert_eq!(
        repo.list_snapshot_deletions().unwrap()[0].snapshot.id,
        old.id
    );
    assert_eq!(
        repo.restore_recycled_snapshot(&items[0].operation_id)
            .unwrap()
            .id,
        restored.id
    );
}

#[test]
fn purging_keeps_the_tombstone_but_prevents_restore() {
    let (_dir, repo, entry, old) = fixture();
    repo.delete_snapshot(&entry, &old.id).unwrap();
    let op = repo.list_snapshot_deletions().unwrap()[0]
        .operation_id
        .clone();
    repo.purge_recycled_snapshot(&op).unwrap();
    repo.purge_recycled_snapshot(&op).unwrap();
    assert!(repo.restore_recycled_snapshot(&op).is_err());
    assert!(repo.list_snapshot_deletions().unwrap()[0].purged);
}

#[test]
fn snapshot_recovery_survives_deleting_its_parent_archive() {
    let (dir, repo, entry, old) = fixture();
    repo.delete_snapshot(&entry, &old.id).unwrap();
    let op = repo.list_snapshot_deletions().unwrap()[0]
        .operation_id
        .clone();
    repo.delete_entry(&entry, None).unwrap();
    let restored = repo.restore_recycled_snapshot(&op).unwrap();
    assert_ne!(restored.id, old.id);
    let reopened = LocalRepository::open(dir.path().join("repo")).unwrap();
    assert!(reopened.verify_snapshot(&restored.id).unwrap());
}

#[test]
fn missing_corrupt_or_locked_snapshot_never_generates_a_deletion() {
    let (_dir, repo, entry, snapshot) = fixture();
    repo.set_snapshot_locked(&entry, &snapshot.id, true)
        .unwrap();
    assert!(repo.delete_snapshot(&entry, &snapshot.id).is_err());
    repo.set_snapshot_locked(&entry, &snapshot.id, false)
        .unwrap();
    let path = repo
        .entry_storage_path(&entry)
        .unwrap()
        .join(snapshot.archive_name);
    fs::write(&path, "corrupt").unwrap();
    assert!(repo.delete_snapshot(&entry, &snapshot.id).is_err());
    fs::remove_file(path).unwrap();
    assert!(repo.delete_snapshot(&entry, &snapshot.id).is_err());
    assert!(repo.list_snapshot_deletions().unwrap().is_empty());
    assert_eq!(repo.list_snapshots(&entry).unwrap().len(), 1);
}

#[test]
fn cloud_import_rejects_windows_alias_and_device_paths() {
    let (_dir, repo, entry, snapshot) = fixture();
    let bytes = fs::read(
        repo.entry_storage_path(&entry)
            .unwrap()
            .join(&snapshot.archive_name),
    )
    .unwrap();
    let catalog: serde_json::Value =
        serde_json::from_slice(&fs::read(repo.root().join("catalog.json")).unwrap()).unwrap();
    let metadata = catalog["entries"][0].clone();
    repo.import_cloud_snapshot(metadata.clone(), snapshot.clone(), &bytes)
        .unwrap();
    for name in [".. ", "CON", "nul.txt", "snapshot.7z.", "snapshot.7z "] {
        let mut node = snapshot.clone();
        node.id = uuid::Uuid::new_v4().to_string();
        node.archive_name = name.into();
        assert!(
            repo.import_cloud_snapshot(metadata.clone(), node, &bytes)
                .is_err(),
            "{name}"
        );
    }
}

#[test]
fn retention_records_its_reason_and_reopen_preserves_recovery() {
    let (_dir, repo, entry, snapshot) = fixture();
    repo.delete_snapshot_with_reason(&entry, &snapshot.id, "retention")
        .unwrap();
    let reopened = LocalRepository::open(repo.root()).unwrap();
    let deletion = &reopened.list_snapshot_deletions().unwrap()[0];
    assert_eq!(deletion.reason, "retention");
    assert_eq!(deletion.device.id, repo.device_identity().unwrap().0);
    assert!(
        reopened
            .restore_recycled_snapshot(&deletion.operation_id)
            .is_ok()
    );
}

#[test]
fn interrupted_delete_and_restore_resume_from_durable_records() {
    let (_dir, repo, entry, snapshot) = fixture();
    repo.delete_snapshot(&entry, &snapshot.id).unwrap();
    let operation = repo.list_snapshot_deletions().unwrap()[0].clone();
    let path = repo
        .root()
        .join("config/snapshot-deletions")
        .join(format!("{}.json", operation.operation_id));
    let mut record = serde_json::to_value(&operation).unwrap();
    record["committed"] = false.into();
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    let reopened = LocalRepository::open(repo.root()).unwrap();
    assert!(reopened.list_snapshots(&entry).unwrap().is_empty());
    let restored = reopened
        .restore_recycled_snapshot(&operation.operation_id)
        .unwrap();
    record = serde_json::to_value(&reopened.list_snapshot_deletions().unwrap()[0]).unwrap();
    record["restoreCommitted"] = false.into();
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    let reopened = LocalRepository::open(repo.root()).unwrap();
    assert_eq!(reopened.list_snapshots(&entry).unwrap().len(), 1);
    assert!(reopened.verify_snapshot(&restored.id).unwrap());
}

#[test]
fn forged_recovery_archive_path_cannot_escape_the_archive_directory() {
    let (_dir, repo, entry, snapshot) = fixture();
    repo.delete_snapshot(&entry, &snapshot.id).unwrap();
    let operation = repo.list_snapshot_deletions().unwrap()[0].clone();
    let recycled = repo
        .root()
        .join("recycle-snapshots")
        .join(format!("{}.7z", operation.operation_id));
    let victim = repo.root().join("victim.7z");
    fs::rename(&recycled, &victim).unwrap();
    let mut record = serde_json::to_value(operation).unwrap();
    record["committed"] = false.into();
    record["snapshot"]["archive_name"] = "../../victim.7z".into();
    let path = repo
        .root()
        .join("config/snapshot-deletions")
        .join(format!("{}.json", record["operationId"].as_str().unwrap()));
    fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(LocalRepository::open(repo.root()).is_err());
    assert!(victim.exists());
}

#[test]
fn imported_deletion_preserves_initiator_and_cannot_remove_locked_content() {
    let (_dir, repo, entry, snapshot) = fixture();
    repo.delete_snapshot(&entry, &snapshot.id).unwrap();
    let mut deletion = repo.list_snapshot_deletions().unwrap()[0].clone();
    let bytes = fs::read(
        repo.root()
            .join("recycle-snapshots")
            .join(format!("{}.7z", deletion.operation_id)),
    )
    .unwrap();
    let restored = repo
        .restore_recycled_snapshot(&deletion.operation_id)
        .unwrap();
    deletion.operation_id = uuid::Uuid::new_v4().to_string();
    deletion.snapshot = restored.clone();
    deletion.device.id = "other-device".into();
    deletion.restored_snapshot = None;
    deletion.restore_committed = false;
    repo.set_snapshot_locked(&entry, &restored.id, true)
        .unwrap();
    assert!(
        repo.accept_snapshot_deletion(deletion.clone(), &bytes)
            .is_err()
    );
    repo.set_snapshot_locked(&entry, &restored.id, false)
        .unwrap();
    deletion.snapshot = repo.get_snapshot(&restored.id).unwrap();
    repo.accept_snapshot_deletion(deletion.clone(), &bytes)
        .unwrap();
    assert!(repo.list_snapshots(&entry).unwrap().is_empty());
    assert!(
        repo.list_snapshot_deletions()
            .unwrap()
            .iter()
            .any(|item| item.device.id == "other-device")
    );
}

#[test]
fn download_import_is_verified_idempotent_and_cannot_resurrect_known_deletions() {
    let (_dir, repo, entry, snapshot) = fixture();
    let bytes = fs::read(
        repo.entry_storage_path(&entry)
            .unwrap()
            .join(&snapshot.archive_name),
    )
    .unwrap();
    let catalog: serde_json::Value =
        serde_json::from_slice(&fs::read(repo.root().join("catalog.json")).unwrap()).unwrap();
    let target = tempfile::tempdir().unwrap();
    let target_repo = LocalRepository::open(target.path()).unwrap();
    let metadata = catalog["entries"][0].clone();
    assert!(
        target_repo
            .import_cloud_snapshot(metadata.clone(), snapshot.clone(), b"corrupt")
            .is_err()
    );
    target_repo
        .import_cloud_snapshot(metadata.clone(), snapshot.clone(), &bytes)
        .unwrap();
    target_repo
        .import_cloud_snapshot(metadata.clone(), snapshot.clone(), &bytes)
        .unwrap();
    assert_eq!(target_repo.list_snapshots(&entry).unwrap().len(), 1);
    target_repo.delete_snapshot(&entry, &snapshot.id).unwrap();
    assert!(
        target_repo
            .import_cloud_snapshot(metadata, snapshot, &bytes)
            .is_err()
    );
}
