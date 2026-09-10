# Update Checking and Restore Suppression Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent restore operations from triggering auto backups and add safe stable/beta update checking with Windows installer launch.

**Architecture:** Rust owns restore-event suppression and trusted installer download/launch. Vue owns GitHub Release discovery, SemVer filtering, About controls, and the update dialog. Settings remain format version 3 and preserve existing settings.

**Tech Stack:** Rust 2024, Tauri 2, Vue 3, Vitest, GitHub Releases REST API, SHA-256.

**Spec:** `docs/superpowers/specs/2026-09-10-update-and-restore-design.md`

## Global Constraints

- Keep settings `formatVersion: 3`; `updateChannel` defaults to `stable`.
- Never create an auto snapshot from restore file events.
- Query only `ThermalEX/Chronicle` Releases; direct installation accepts only its x64 NSIS release asset.
- Direct installation is user-initiated, verifies `SHA256SUMS.txt` when supplied, then launches the installer and exits Chronicle.
- Do not create a Release or build a distributable during this task.

---

### Task 1: Restore event suppression

**Files:**
- Modify: `apps/desktop/src-tauri/src/auto_backup.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Test: `apps/desktop/src-tauri/src/auto_backup.rs`

**Interfaces:**
- Produces `begin_restore_suppression`, `finish_restore_suppression`, and `cancel_restore_suppression` on `AutoBackupManager`.
- Consumes `autoBackupDelaySeconds` to retain suppression after a successful restore.

- [ ] **Step 1: Write failing suppression tests**

```rust
#[test]
fn restore_events_are_ignored_until_the_merge_window_ends() {
    let mut suppressions = HashMap::new();
    begin_restore_suppression(&mut suppressions, "entry");
    assert!(is_restore_suppressed(&mut suppressions, "entry", Instant::now()));
    finish_restore_suppression(&mut suppressions, "entry", Instant::now(), Duration::from_secs(5));
    assert!(is_restore_suppressed(&mut suppressions, "entry", Instant::now()));
}
```

- [ ] **Step 2: Run `cargo test auto_backup::tests` and confirm the missing helper failure.**
- [ ] **Step 3: Add the suppression map and helpers; clear pending work at restore start; ignore suppressed watcher events. Call begin before `repository.restore_snapshot`, finish only on success, cancel on failure.**
- [ ] **Step 4: Run `cargo test auto_backup::tests` and confirm success.**

### Task 2: Update channel, release selection, and settings migration

**Files:**
- Create: `apps/desktop/src/services/updateService.ts`
- Create: `apps/desktop/src/services/updateService.test.ts`
- Modify: `apps/desktop/src/services/settings.ts`
- Modify: `apps/desktop/src/services/settings.test.ts`

**Interfaces:**
- Produces `checkForUpdate(channel, currentVersion)`, `selectUpdate`, and `ReleaseUpdate`.
- Consumes GitHub Release JSON and returns only a strictly newer matching release.

- [ ] **Step 1: Write failing tests for default `updateChannel`, stable-channel prerelease filtering, beta-channel selection, and equal-version suppression.**
- [ ] **Step 2: Run `npm test -- --run src/services/settings.test.ts src/services/updateService.test.ts` and confirm the new service test fails.**
- [ ] **Step 3: Add `UpdateChannel`, normalize it to `stable`, fetch `https://api.github.com/repos/ThermalEX/Chronicle/releases`, parse SemVer, select the matching newer release, its changelog, release URL, x64 installer, and optional SHA256SUMS asset.**
- [ ] **Step 4: Run the two tests and confirm success.**

### Task 3: Trusted installer downloader and launcher

**Files:**
- Create: `apps/desktop/src-tauri/src/update.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Test: `apps/desktop/src-tauri/src/update.rs`

**Interfaces:**
- Produces command `download_and_install_update(url, file_name, sha256_sums_url)`.
- Rejects every URL outside `github.com/ThermalEX/Chronicle/releases/download/` and non-NSIS filenames.

- [ ] **Step 1: Write failing URL/asset validation tests.**
- [ ] **Step 2: Run `cargo test update::tests` and confirm failure.**
- [ ] **Step 3: Add a Tauri command that downloads to a unique file under the Windows Downloads directory, validates `SHA256SUMS.txt` when present, atomically publishes the installer, starts it, and calls `app.exit(0)`. Register the module and command.**
- [ ] **Step 4: Run `cargo test update::tests` and confirm success.**

### Task 4: About controls and update dialog

**Files:**
- Create: `apps/desktop/src/components/UpdateDialog.vue`
- Modify: `apps/desktop/src/components/SettingsDialog.vue`
- Modify: `apps/desktop/src/App.vue`
- Modify: `apps/desktop/src/styles.css`

**Interfaces:**
- `SettingsDialog` emits `check-update`; `App.vue` owns `ReleaseUpdate | undefined` and invokes the update service on startup or manual request.
- `UpdateDialog` accepts `update` and invokes browser, copy, or native install actions.

- [ ] **Step 1: Write a failing component/service test that checks stable/beta controls are in About and a matching update opens the dialog.**
- [ ] **Step 2: Run the targeted Vitest test and confirm failure.**
- [ ] **Step 3: Move the automatic-check switch to About; add channel select and manual check. Add a modal with close button, readable release body, copy/browser/download-install actions, disabled/loading states, visible focus, and semantic labels. Run startup checks silently.**
- [ ] **Step 4: Run targeted tests and confirm success.**

### Task 5: Verification

- [ ] **Step 1: Run `npm test -- --run` and `npm run build` in `apps/desktop`.**
- [ ] **Step 2: Run `cargo test --workspace` and `cargo check` in `apps/desktop/src-tauri`.**
- [ ] **Step 3: Run `git diff --check`; do not publish a Release.**
