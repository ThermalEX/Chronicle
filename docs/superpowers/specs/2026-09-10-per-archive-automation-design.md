# Per-Archive Automation Design

## Goal

Move automatic backup and upload decisions from application settings to each archive, while keeping the backup quiet period as an application-wide setting.

## Behavior

- Every archive stores `autoBackupEnabled` and `automaticUploadEnabled`, both defaulting to `false` for migrations and new archives.
- Auto backup watches an archive's local sources and creates a snapshot after the configured quiet period.
- Auto upload uploads every newly-created snapshot for an archive whose auto-upload option is enabled and whose storage policy includes the cloud.
- Enabling both options makes a source change create and then upload a snapshot. Enabling either option alone retains its independent behavior.
- Old application-wide automation booleans are ignored on read and removed when settings are next saved; the quiet period remains global.

## Interface

- The create/edit archive dialog shows both switches with clear descriptions.
- Archive names and their automation badges open that archive's edit dialog.
- The backup settings section removes the two global switches and the local-library status card. It provides batch enable/disable actions for each archive-level automation option.
- Badges communicate enabled auto backup and auto upload without replacing the archive name.

## Validation

- Storage and browser-repository tests cover default and persisted archive automation values.
- Settings migration tests cover removal of global automation flags while retaining the quiet-period default.
- Rust tests cover persistence and batch mutation of both flags.
- Desktop frontend build and Rust tests compile the updated data flow.
