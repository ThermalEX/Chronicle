use chronicle_storage::LocalRepository;
use std::fs;

#[test]
fn rename_is_persistent_and_keeps_identity_and_historical_snapshot_name() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    let repo = LocalRepository::open(&repo_path).unwrap();
    let original = repo.device_identity().unwrap();
    let source = dir.path().join("save.txt");
    fs::write(&source, "save").unwrap();
    let entry = repo.add_entry(&source, None, None).unwrap();
    let snapshot = repo
        .create_snapshot(&entry.id, "first", &original.0, false)
        .unwrap();
    repo.rename_device("  游戏电脑  ").unwrap();
    let reopened = LocalRepository::open(repo_path).unwrap();
    assert_eq!(
        reopened.device_identity().unwrap(),
        (original.0, "游戏电脑".into())
    );
    assert_eq!(
        reopened.list_snapshots(&entry.id).unwrap()[0].device_name,
        snapshot.device_name
    );
}

#[test]
fn invalid_names_leave_device_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let repo = LocalRepository::open(dir.path()).unwrap();
    let original = repo.device_identity().unwrap();
    for name in ["   ".to_owned(), "字".repeat(65)] {
        assert!(repo.rename_device(&name).is_err());
        assert_eq!(repo.device_identity().unwrap(), original);
    }
    repo.rename_device(&"字".repeat(64)).unwrap();
}

#[test]
fn new_device_identity_preserves_archives_snapshots_and_source_bindings() {
    let dir = tempfile::tempdir().unwrap();
    let repo = LocalRepository::open(dir.path().join("repo")).unwrap();
    repo.rename_device("Laptop").unwrap();
    let old = repo.device_identity().unwrap();
    let source = dir.path().join("save.txt");
    fs::write(&source, "save").unwrap();
    let entry = repo.add_entry(&source, None, None).unwrap();
    let snapshot = repo
        .create_snapshot(&entry.id, "first", &old.0, false)
        .unwrap();
    let bindings = fs::read(repo.root().join("config/bindings.json")).unwrap();
    repo.reset_device_identity().unwrap();
    let new = repo.device_identity().unwrap();
    assert_ne!(old.0, new.0);
    assert_eq!(old.1, new.1);
    assert_eq!(repo.list_snapshots(&entry.id).unwrap(), vec![snapshot]);
    assert_eq!(
        fs::read(repo.root().join("config/bindings.json")).unwrap(),
        bindings
    );
}
