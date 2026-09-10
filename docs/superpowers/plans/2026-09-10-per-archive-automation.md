# Per-Archive Automation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let each Chronicle archive opt into automatic backup and automatic upload independently.

**Architecture:** Persist both flags alongside each core `Entry` and catalog summary, expose them through the Tauri repository API, and use them in the native watcher and snapshot event handler. Keep only the quiet delay in application settings. The Vue dialog and archive list become the visible per-archive controls, while settings offers explicit bulk actions.

**Tech Stack:** Vue 3 + TypeScript, Tauri 2 + Rust, chronicle-core, chronicle-storage, Vitest, Cargo tests.

**Spec:** `docs/superpowers/specs/2026-09-10-per-archive-automation-design.md`

## Global Constraints

- Both archive-level automation flags default to `false` for old and new data.
- `autoBackupDelaySeconds` remains global and is clamped to 1–300 seconds.
- Automatic upload only runs for cloud-backed archives and does not remove a newly-created local snapshot after failure.
- Keep the existing Chinese copy style and semantic theme tokens.

---

### Task 1: Persist archive-level auto-upload state

**Files:**
- Modify: `crates/chronicle-core/src/lib.rs`
- Modify: `crates/chronicle-storage/src/repository.rs`
- Modify: `crates/chronicle-storage/src/repository.rs` tests

**Interfaces:**
- Produces `Entry.automatic_upload_enabled: bool` and `LocalRepository::set_entry_automation(entry_id, auto_backup_enabled, automatic_upload_enabled)`.

- [ ] **Step 1: Write a failing storage test**

```rust
#[test]
fn automation_settings_survive_entry_updates() {
    let updated = repository.set_entry_automation(&entry.id, true, true).unwrap();
    assert!(updated.auto_backup_enabled);
    assert!(updated.automatic_upload_enabled);
}
```

- [ ] **Step 2: Run the storage test and verify it fails**

Run: `cargo test automation_settings_survive_entry_updates`

- [ ] **Step 3: Add the serde-defaulted flag to Entry and CatalogEntry, map it on creation/read/update, and add the mutation method.**

- [ ] **Step 4: Run the storage tests and verify they pass**

Run: `cargo test`

### Task 2: Expose automation state to desktop clients

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src/domain.ts`
- Modify: `apps/desktop/src/services/repository.ts`
- Modify: `apps/desktop/src/services/archiveRepository.ts`
- Test: `apps/desktop/src/services/archiveRepository.test.ts`

**Interfaces:**
- Produces `ArchiveRecord.automaticUploadEnabled`, `CreateArchiveInput.automaticUploadEnabled`, and `setArchiveAutomation(id, backup, upload)`.

- [ ] **Step 1: Write a failing browser repository test that creates and lists an archive with both options true.**

- [ ] **Step 2: Run `npm test -- --run src/services/archiveRepository.test.ts` and verify it fails.**

- [ ] **Step 3: Thread both flags through TypeScript records, browser storage, Tauri DTOs, and the Tauri command.**

- [ ] **Step 4: Run the focused Vitest suite and verify it passes.**

### Task 3: Move automation execution to archive flags

**Files:**
- Modify: `apps/desktop/src-tauri/src/auto_backup.rs`
- Modify: `apps/desktop/src/App.vue`
- Modify: `apps/desktop/src/services/settings.ts`
- Modify: `apps/desktop/src/services/settings.test.ts`

**Interfaces:**
- Consumes archive flags from Task 2.
- Produces watcher eligibility based solely on `Entry.auto_backup_enabled` and uploads based solely on `ArchiveRecord.automaticUploadEnabled`.

- [ ] **Step 1: Write failing settings tests asserting global automation flags are not normalized and the five-second default remains.**

- [ ] **Step 2: Run `npm test -- --run src/services/settings.test.ts` and verify it fails.**

- [ ] **Step 3: Remove global automation fields and controls, update the watcher to read per-entry backup state, and trigger snapshot upload through the per-entry upload state.**

- [ ] **Step 4: Run settings tests, `cargo test --lib`, and the desktop build.**

### Task 4: Add per-archive controls and status badges

**Files:**
- Modify: `apps/desktop/src/components/CreateArchiveDialog.vue`
- Modify: `apps/desktop/src/App.vue`
- Modify: archive-list styles in `apps/desktop/src/App.vue`

**Interfaces:**
- Consumes `autoBackupEnabled` and `automaticUploadEnabled`.
- Produces click targets for archive names and automation badges that open the same edit dialog.

- [ ] **Step 1: Write a failing component-level assertion or focused source test for the two dialog controls and edit trigger.**

- [ ] **Step 2: Run the focused check and verify it fails.**

- [ ] **Step 3: Add the two dialog switches with descriptions, show compact semantic badges next to archive names, and route both name and badge clicks to the archive edit action.**

- [ ] **Step 4: Run `npm run build` and inspect keyboard focus and badge text overflow.**

### Task 5: Add bulk controls in backup settings

**Files:**
- Modify: `apps/desktop/src/components/SettingsDialog.vue`
- Modify: `apps/desktop/src/App.vue`
- Test: `apps/desktop/src/services/repository.test.ts`

**Interfaces:**
- Consumes `setArchiveAutomation` from Task 2.
- Produces bulk enable/disable actions for backup and upload, with a refresh of watcher registrations after backup changes.

- [ ] **Step 1: Write a failing repository test for updating an archive automation flag.**

- [ ] **Step 2: Run `npm test -- --run src/services/repository.test.ts` and verify it fails.**

- [ ] **Step 3: Remove the local-library card, add two compact bulk action groups, and wire each action through the repository service.**

- [ ] **Step 4: Run all frontend tests, `cargo test --lib`, `cargo test` in `crates/chronicle-storage`, and `npm run build`.**
