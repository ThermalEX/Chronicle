use chronicle_core::{ChangeSummary, Snapshot};
use chronicle_sync::snapshot_protocol::{Action, Device, Event, EventKind, plan_sync};

fn snapshot(id: &str) -> Snapshot {
    Snapshot {
        id: id.into(),
        entry_id: "game".into(),
        parent_id: None,
        device_id: "a".into(),
        device_name: "PC".into(),
        title: id.into(),
        note: String::new(),
        created_at_ms: 1,
        archive_name: format!("{id}.7z"),
        object_hash: "hash".into(),
        size_bytes: 1,
        files: vec![],
        changes: ChangeSummary::default(),
        safety: false,
        locked: false,
        metadata_updated_at_ms: 0,
        exclude_patterns: vec![],
    }
}
fn publication(snapshot: Snapshot) -> Event {
    Event {
        operation_id: format!("publish-{}", snapshot.id),
        device: Device {
            id: "a".into(),
            name: "PC".into(),
            revision: 0,
        },
        kind: EventKind::Published {
            snapshot,
            object_path: "archives/game/file.7z".into(),
            entry: serde_json::Value::Null,
        },
    }
}
fn deletion(snapshot: Snapshot) -> Event {
    Event {
        operation_id: format!("delete-{}", snapshot.id),
        device: Device {
            id: "b".into(),
            name: "Laptop".into(),
            revision: 0,
        },
        kind: EventKind::Deleted {
            snapshot,
            reason: "manual".into(),
            observed_revisions: vec![],
            object_path: "archives/game/file.7z".into(),
            deleted_at_ms: 1,
            related_revisions: vec![],
            entry: serde_json::Value::Null,
        },
    }
}

#[test]
fn concurrent_new_snapshots_merge_and_deleted_snapshot_does_not_resurrect() {
    let old = snapshot("old");
    let events = vec![
        publication(old.clone()),
        deletion(old.clone()),
        publication(snapshot("b-new")),
    ];
    let plan = plan_sync(&[old, snapshot("a-new")], &events, &[], false);
    assert!(
        plan.iter()
            .any(|item| item.snapshot.id == "a-new" && item.action == Action::Upload)
    );
    assert!(
        plan.iter()
            .any(|item| item.snapshot.id == "b-new" && item.action == Action::Download)
    );
    assert!(plan.iter().any(|item| item.snapshot.id == "old"
        && item.action == Action::RecycleLocal
        && !item.selected));
    assert!(!plan.iter().any(|item| item.snapshot.id == "old"
        && matches!(item.action, Action::Upload | Action::Download)));
}
#[test]
fn automatic_upload_neither_downloads_nor_propagates_pending_deletes() {
    let old = snapshot("old");
    let plan = plan_sync(
        &[old.clone(), snapshot("new")],
        &[deletion(old.clone()), publication(snapshot("remote"))],
        &[deletion(snapshot("pending"))],
        true,
    );
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].snapshot.id, "new");
    assert_eq!(plan[0].action, Action::Upload);
}
#[test]
fn a_missing_local_file_is_not_a_delete_and_pending_deletes_need_explicit_selection() {
    let old = snapshot("old");
    assert_eq!(
        plan_sync(&[], &[publication(old.clone())], &[], false)[0].action,
        Action::Download
    );
    let plan = plan_sync(&[], &[publication(old.clone())], &[deletion(old)], false);
    assert_eq!(plan[0].action, Action::RecycleRemote);
    assert!(!plan[0].selected);
}
#[test]
fn concurrent_lock_or_metadata_edit_conflicts_with_delete_without_clock_comparison() {
    let old = snapshot("old");
    let mut locked = old.clone();
    locked.locked = true;
    locked.metadata_updated_at_ms = 1;
    let plan = plan_sync(
        &[locked],
        &[publication(old.clone()), deletion(old)],
        &[],
        false,
    );
    assert_eq!(plan[0].action, Action::Conflict);
    assert!(!plan[0].selected);
}
#[test]
fn duplicate_events_are_idempotent_and_remote_lock_always_preserves_content() {
    let mut old = snapshot("old");
    old.locked = true;
    let delete = deletion(snapshot("old"));
    let plan = plan_sync(&[], &[publication(old), delete.clone(), delete], &[], false);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].action, Action::Conflict);
}

#[test]
fn conflicting_publications_or_duplicate_operation_ids_fail_closed() {
    let old = snapshot("old");
    let original = publication(old.clone());
    let mut edited = old;
    edited.note = "other".into();
    let mut conflicting = publication(edited);
    assert_eq!(
        plan_sync(&[], &[original.clone(), conflicting.clone()], &[], false)[0].action,
        Action::Conflict
    );
    conflicting.operation_id = "another-publish".into();
    assert_eq!(
        plan_sync(&[], &[original, conflicting], &[], false)[0].action,
        Action::Conflict
    );
}

#[test]
fn cyclic_or_content_changing_revisions_are_conflicts() {
    let old = snapshot("old");
    let device = publication(old.clone()).device;
    let cyclic = vec![
        Event {
            operation_id: "r1".into(),
            device: device.clone(),
            kind: EventKind::Revised {
                snapshot: old.clone(),
                parents: vec!["r2".into()],
            },
        },
        Event {
            operation_id: "r2".into(),
            device: device.clone(),
            kind: EventKind::Revised {
                snapshot: old.clone(),
                parents: vec!["r1".into()],
            },
        },
        publication(old.clone()),
    ];
    assert_eq!(
        plan_sync(std::slice::from_ref(&old), &cyclic, &[], false)[0].action,
        Action::Conflict
    );
    let mut changed = old.clone();
    changed.object_hash = "other content".into();
    let records = vec![
        publication(old.clone()),
        Event {
            operation_id: "r3".into(),
            device,
            kind: EventKind::Revised {
                snapshot: changed,
                parents: vec![],
            },
        },
    ];
    assert_eq!(
        plan_sync(&[old], &records, &[], false)[0].action,
        Action::Conflict
    );
}

#[test]
fn known_device_names_use_revisions_and_never_guess_missing_identity() {
    use chronicle_sync::snapshot_protocol::known_devices;
    let mut old = publication(snapshot("old"));
    old.device.name = "Original".into();
    let renamed = Event {
        operation_id: "name-1".into(),
        device: Device {
            id: "a".into(),
            name: "Desktop".into(),
            revision: 1,
        },
        kind: EventKind::DeviceNamed,
    };
    let same_name = Event {
        operation_id: "name-b".into(),
        device: Device {
            id: "b".into(),
            name: "Desktop".into(),
            revision: 0,
        },
        kind: EventKind::DeviceSynced { at_ms: 123 },
    };
    let devices = known_devices(&[old, renamed, same_name]);
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].name, "Desktop");
    assert_eq!(devices[0].last_successful_sync_at, None);
    assert_eq!(devices[1].last_successful_sync_at, Some(123));
}

#[test]
fn causal_metadata_revision_uploads_and_downloads_without_using_clocks() {
    let old = snapshot("old");
    let mut changed = old.clone();
    changed.locked = true;
    let edit = Event {
        operation_id: "edit-1".into(),
        device: Device {
            id: "a".into(),
            name: "PC".into(),
            revision: 0,
        },
        kind: EventKind::Revised {
            snapshot: changed.clone(),
            parents: vec![],
        },
    };
    assert_eq!(
        plan_sync(
            &[changed.clone()],
            &[publication(old.clone())],
            std::slice::from_ref(&edit),
            false
        )[0]
        .action,
        Action::UploadRevision
    );
    assert_eq!(
        plan_sync(&[old], &[publication(snapshot("old")), edit], &[], false)[0].action,
        Action::DownloadRevision
    );
}
