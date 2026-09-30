use chronicle_storage::LocalRepository;
use std::fs;
#[test]
fn edits_record_causal_parents_and_deletion_captures_observed_revisions() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("save.txt");
    fs::write(&source, "save").unwrap();
    let repo = LocalRepository::open(dir.path().join("repo")).unwrap();
    let entry = repo.add_entry(source, None, None).unwrap();
    let snapshot = repo
        .create_snapshot(&entry.id, "first", "device", false)
        .unwrap();
    repo.update_snapshot_note(&entry.id, &snapshot.id, "note")
        .unwrap();
    let first = repo.snapshot_revision_records().unwrap();
    assert_eq!(first.len(), 1);
    let first_id = first[0]["operationId"].as_str().unwrap().to_owned();
    repo.set_snapshot_locked(&entry.id, &snapshot.id, false)
        .unwrap();
    let records = repo.snapshot_revision_records().unwrap();
    assert!(records.iter().any(|record| {
        record["kind"]["parents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some(&first_id))
    }));
    repo.delete_snapshot(&entry.id, &snapshot.id).unwrap();
    assert!(
        repo.list_snapshot_deletions().unwrap()[0]
            .observed_revisions
            .contains(&first_id)
    );
}
