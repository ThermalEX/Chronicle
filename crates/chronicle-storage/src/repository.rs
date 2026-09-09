use std::{
    cmp::Reverse,
    collections::HashMap,
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use chronicle_core::{
    Category, ChangeSummary, Entry, EntryKind, EntrySource, Snapshot, SnapshotFile, StoragePolicy,
    SyncMode,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sevenz_rust::{compress_to_path, decompress_file};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;
use walkdir::WalkDir;

const FORMAT_VERSION: u32 = 4;
const SETTINGS_VERSION: u32 = 3;
const HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("metadata error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("7z archive error: {0}")]
    SevenZip(#[from] sevenz_rust::Error),
    #[error("directory scan error: {0}")]
    Walk(#[from] walkdir::Error),
    #[error("source does not exist: {0}")]
    MissingSource(PathBuf),
    #[error("source path has no file name: {0}")]
    MissingName(PathBuf),
    #[error("path cannot be represented as UTF-8: {0}")]
    NonUtf8Path(PathBuf),
    #[error("an archive requires at least one source")]
    EmptySources,
    #[error("archive name cannot be empty")]
    EmptyName,
    #[error("settings must use formatVersion 3 and contain app and cloud objects")]
    InvalidSettings,
    #[error("category name cannot be empty")]
    EmptyCategoryName,
    #[error("parent category not found: {0}")]
    ParentCategoryNotFound(String),
    #[error("a category with this name already exists at the selected level")]
    DuplicateCategory,
    #[error("a category cannot be moved into itself or one of its descendants")]
    InvalidCategoryMove,
    #[error("recycle item cannot be restored: {0}")]
    RestoreConflict(String),
    #[error("entry not found: {0}")]
    EntryNotFound(String),
    #[error("snapshot not found: {0}")]
    SnapshotNotFound(String),
    #[error("snapshot object failed content verification")]
    IntegrityMismatch,
    #[error("snapshot content is incomplete for source: {0}")]
    IncompleteSnapshot(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Catalog {
    format_version: u32,
    updated_at_ms: u64,
    #[serde(default)]
    categories: Vec<Category>,
    entries: Vec<CatalogEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CatalogEntry {
    id: String,
    name: String,
    folder: String,
    category_id: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    storage_policy: StoragePolicy,
    #[serde(default)]
    sync_mode: SyncMode,
    source_count: usize,
    snapshot_count: usize,
    stored_bytes: u64,
    last_snapshot_at_ms: Option<u64>,
    #[serde(default)]
    sources: Vec<EntrySource>,
    #[serde(default)]
    created_at_ms: u64,
    #[serde(default)]
    snapshots: Vec<Snapshot>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct BindingsDocument {
    format_version: u32,
    #[serde(default)]
    entries: HashMap<String, Vec<EntrySource>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DeviceDocument {
    format_version: u32,
    id: String,
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RecycleManifest {
    format_version: u32,
    id: String,
    kind: String,
    display_name: String,
    deleted_at_ms: u64,
    categories: Vec<Category>,
    entries: Vec<CatalogEntry>,
}

/// Summary of one recoverable deletion batch.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RecycleItem {
    /// Stable batch identifier.
    pub id: String,
    /// `entry` or `category`.
    pub kind: String,
    /// User-facing deleted item name.
    pub display_name: String,
    /// Deletion time as Unix milliseconds.
    pub deleted_at_ms: u64,
    /// Total stored bytes in the recycle bundle.
    pub size_bytes: u64,
    /// Number of entries contained in this bundle.
    pub entry_count: usize,
}

/// Filesystem-backed Chronicle repository using JSON metadata and 7z timelines.
#[derive(Clone, Debug)]
pub struct LocalRepository {
    root: PathBuf,
}

impl LocalRepository {
    /// Opens or initializes a repository at `root`.
    ///
    /// # Errors
    /// Returns an error when the repository structure cannot be created.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let repository = Self { root: root.into() };
        repository.initialize()?;
        Ok(repository)
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the total number of bytes currently stored in the repository.
    ///
    /// # Errors
    /// Returns an error when repository files cannot be inspected.
    pub fn total_stored_bytes(&self) -> Result<u64> {
        let mut total = 0_u64;
        for item in WalkDir::new(&self.root) {
            let item = item?;
            if item.file_type().is_file() {
                total = total.saturating_add(item.metadata()?.len());
            }
        }
        Ok(total)
    }

    /// Returns repository bytes plus an external recycle directory without double counting.
    ///
    /// # Errors
    /// Returns an error when either directory cannot be inspected.
    pub fn total_stored_bytes_with_recycle(&self, recycle_root: Option<&Path>) -> Result<u64> {
        let total = self.total_stored_bytes()?;
        match recycle_root {
            Some(path) if !path.starts_with(&self.root) && path.exists() => {
                Ok(total.saturating_add(directory_size(path)?))
            }
            _ => Ok(total),
        }
    }

    /// Reads the versioned application settings document.
    ///
    /// # Errors
    /// Returns an error when the settings file cannot be read or decoded.
    pub fn load_settings(&self) -> Result<Value> {
        read_json(&self.settings_path())
    }

    /// Replaces the application settings document atomically.
    ///
    /// # Errors
    /// Returns an error when the settings file cannot be written.
    pub fn save_settings(&self, settings: &Value) -> Result<()> {
        let valid = settings.get("formatVersion").and_then(Value::as_u64)
            == Some(u64::from(SETTINGS_VERSION))
            && settings.get("app").is_some_and(Value::is_object)
            && settings.get("cloud").is_some_and(Value::is_object);
        if !valid {
            return Err(StorageError::InvalidSettings);
        }
        write_json_atomic(&self.settings_path(), settings, &self.temp_dir())
    }

    /// Registers a single existing path as an entry.
    ///
    /// # Errors
    /// Returns an error when the source cannot be inspected or metadata cannot be written.
    pub fn add_entry(
        &self,
        source_path: impl AsRef<Path>,
        name: Option<&str>,
        category_id: Option<String>,
    ) -> Result<Entry> {
        let source = source_path.as_ref();
        let default_name = source
            .file_name()
            .ok_or_else(|| StorageError::MissingName(source.to_path_buf()))?
            .to_string_lossy()
            .into_owned();
        let name = name.unwrap_or(&default_name);
        self.add_entry_sources(
            name,
            &[path_string(source)?],
            category_id,
            StoragePolicy::Local,
        )
    }

    /// Registers files and directories that form one named archive.
    ///
    /// # Errors
    /// Returns an error when a source is missing or entry metadata cannot be written.
    pub fn add_entry_sources(
        &self,
        name: &str,
        source_paths: &[String],
        category_id: Option<String>,
        storage_policy: StoragePolicy,
    ) -> Result<Entry> {
        self.add_entry_sources_with_sync(
            name,
            source_paths,
            category_id,
            storage_policy,
            SyncMode::Manual,
        )
    }

    /// Registers an entry and its synchronization mode.
    ///
    /// # Errors
    /// Returns an error for invalid sources or failed repository writes.
    pub fn add_entry_sources_with_sync(
        &self,
        name: &str,
        source_paths: &[String],
        category_id: Option<String>,
        storage_policy: StoragePolicy,
        sync_mode: SyncMode,
    ) -> Result<Entry> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::EmptyName);
        }
        if source_paths.is_empty() {
            return Err(StorageError::EmptySources);
        }
        let sources = resolve_entry_sources(source_paths, &[])?;
        let id = Uuid::new_v4().to_string();
        let folder = format!("{}__{id}", safe_folder_name(name));
        let entry = Entry {
            id,
            name: name.to_owned(),
            sources,
            category_id,
            tags: Vec::new(),
            storage_policy,
            sync_mode,
            created_at_ms: unix_millis(SystemTime::now()),
        };
        let directory = self.entries_dir().join(&folder);
        fs::create_dir_all(&directory)?;
        self.save_bindings_for_entry(&entry.id, &entry.sources)?;
        let mut catalog = self.read_catalog()?;
        catalog.entries.push(CatalogEntry {
            id: entry.id.clone(),
            name: entry.name.clone(),
            folder,
            category_id: entry.category_id.clone(),
            tags: entry.tags.clone(),
            storage_policy,
            sync_mode,
            source_count: entry.sources.len(),
            snapshot_count: 0,
            stored_bytes: 0,
            last_snapshot_at_ms: None,
            sources: shared_entry(&entry).sources,
            created_at_ms: entry.created_at_ms,
            snapshots: Vec::new(),
        });
        self.write_catalog(&mut catalog)?;
        Ok(entry)
    }

    /// Updates an entry's display name, sources, and storage policy.
    ///
    /// Existing source identifiers are preserved when their canonical paths are unchanged.
    ///
    /// # Errors
    /// Returns an error for invalid values, missing sources, or metadata write failures.
    pub fn update_entry(
        &self,
        entry_id: &str,
        name: &str,
        source_paths: &[String],
        storage_policy: StoragePolicy,
    ) -> Result<Entry> {
        self.update_entry_with_sync(
            entry_id,
            name,
            source_paths,
            storage_policy,
            SyncMode::Manual,
        )
    }

    /// Updates an entry including its synchronization mode.
    ///
    /// # Errors
    /// Returns an error for invalid sources, a missing entry, or failed writes.
    pub fn update_entry_with_sync(
        &self,
        entry_id: &str,
        name: &str,
        source_paths: &[String],
        storage_policy: StoragePolicy,
        sync_mode: SyncMode,
    ) -> Result<Entry> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::EmptyName);
        }
        if source_paths.is_empty() {
            return Err(StorageError::EmptySources);
        }
        let mut entry = self.get_entry(entry_id)?;
        entry.sources = resolve_entry_sources_by_position(source_paths, &entry.sources)?;
        name.clone_into(&mut entry.name);
        entry.storage_policy = storage_policy;
        entry.sync_mode = sync_mode;
        self.save_bindings_for_entry(&entry.id, &entry.sources)?;
        let mut catalog = self.read_catalog()?;
        let summary = catalog
            .entries
            .iter_mut()
            .find(|item| item.id == entry_id)
            .ok_or_else(|| StorageError::EntryNotFound(entry_id.into()))?;
        summary.name.clone_from(&entry.name);
        summary.storage_policy = storage_policy;
        summary.sync_mode = sync_mode;
        summary.source_count = entry.sources.len();
        summary.sources = shared_entry(&entry).sources;
        self.write_catalog(&mut catalog)?;
        Ok(entry)
    }

    /// Changes only an entry's synchronization policy without touching its sources or snapshots.
    ///
    /// # Errors
    /// Returns an error when the entry does not exist or its metadata cannot be persisted.
    pub fn set_entry_sync_mode(&self, entry_id: &str, sync_mode: SyncMode) -> Result<Entry> {
        let mut entry = self.get_entry(entry_id)?;
        entry.sync_mode = sync_mode;

        let mut catalog = self.read_catalog()?;
        let summary = catalog
            .entries
            .iter_mut()
            .find(|item| item.id == entry_id)
            .ok_or_else(|| StorageError::EntryNotFound(entry_id.into()))?;
        summary.sync_mode = sync_mode;
        self.write_catalog(&mut catalog)?;
        Ok(entry)
    }

    /// Removes one entry permanently or moves its complete directory into a recycle bin.
    ///
    /// # Errors
    /// Returns an error when the entry is missing or its data cannot be moved or deleted.
    pub fn delete_entry(&self, entry_id: &str, recycle_root: Option<&Path>) -> Result<()> {
        let mut catalog = self.read_catalog()?;
        let summary = catalog
            .entries
            .iter()
            .find(|item| item.id == entry_id)
            .cloned()
            .ok_or_else(|| StorageError::EntryNotFound(entry_id.into()))?;
        let source = self.entries_dir().join(&summary.folder);
        let recycle_batch = if let Some(root) = recycle_root {
            let batch = recycle_batch(root, "entry", entry_id)?;
            copy_directory_verified(&source, &batch.join(&summary.folder))?;
            write_json_atomic(
                &batch.join("manifest.json"),
                &RecycleManifest {
                    format_version: 1,
                    id: batch
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    kind: "entry".into(),
                    display_name: summary.name.clone(),
                    deleted_at_ms: unix_millis(SystemTime::now()),
                    categories: Vec::new(),
                    entries: vec![summary.clone()],
                },
                &self.temp_dir(),
            )?;
            Some(batch)
        } else {
            None
        };
        let staged_root = self
            .temp_dir()
            .join(format!("delete-entry-{}", Uuid::new_v4()));
        fs::create_dir_all(&staged_root)?;
        let staged_source = staged_root.join(&summary.folder);
        if source.exists()
            && let Err(error) = fs::rename(&source, &staged_source)
        {
            let _ = fs::remove_dir_all(&staged_root);
            if let Some(batch) = recycle_batch {
                let _ = fs::remove_dir_all(batch);
            }
            return Err(error.into());
        }
        catalog.entries.retain(|item| item.id != entry_id);
        if let Err(error) = self.write_catalog(&mut catalog) {
            if staged_source.exists() {
                let _ = fs::rename(&staged_source, &source);
            }
            let _ = fs::remove_dir_all(&staged_root);
            if let Some(batch) = recycle_batch {
                let _ = fs::remove_dir_all(batch);
            }
            return Err(error);
        }
        if staged_root.exists() {
            fs::remove_dir_all(staged_root)?;
        }
        Ok(())
    }

    /// Removes a category subtree and every entry assigned within it.
    ///
    /// # Errors
    /// Returns an error when the category is missing or its data cannot be moved or deleted.
    pub fn delete_category(&self, category_id: &str, recycle_root: Option<&Path>) -> Result<()> {
        let mut catalog = self.read_catalog()?;
        if !catalog.categories.iter().any(|item| item.id == category_id) {
            return Err(StorageError::ParentCategoryNotFound(category_id.into()));
        }
        let mut category_ids = vec![category_id.to_owned()];
        let mut cursor = 0;
        while cursor < category_ids.len() {
            let parent = category_ids[cursor].clone();
            category_ids.extend(
                catalog
                    .categories
                    .iter()
                    .filter(|item| item.parent_id.as_ref() == Some(&parent))
                    .map(|item| item.id.clone()),
            );
            cursor += 1;
        }
        let removed_entries = catalog
            .entries
            .iter()
            .filter(|item| {
                item.category_id
                    .as_ref()
                    .is_some_and(|id| category_ids.contains(id))
            })
            .cloned()
            .collect::<Vec<_>>();
        let removed_categories = catalog
            .categories
            .iter()
            .filter(|item| category_ids.contains(&item.id))
            .cloned()
            .collect::<Vec<_>>();
        let recycle_batch = if let Some(root) = recycle_root {
            let batch = recycle_batch(root, "category", category_id)?;
            for entry in &removed_entries {
                copy_directory_verified(
                    &self.entries_dir().join(&entry.folder),
                    &batch.join(&entry.folder),
                )?;
            }
            let display_name = removed_categories
                .iter()
                .find(|item| item.id == category_id)
                .map_or_else(|| category_id.to_owned(), |item| item.name.clone());
            write_json_atomic(
                &batch.join("manifest.json"),
                &RecycleManifest {
                    format_version: 1,
                    id: batch
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    kind: "category".into(),
                    display_name,
                    deleted_at_ms: unix_millis(SystemTime::now()),
                    categories: removed_categories.clone(),
                    entries: removed_entries.clone(),
                },
                &self.temp_dir(),
            )?;
            Some(batch)
        } else {
            None
        };
        let staged_root = self
            .temp_dir()
            .join(format!("delete-category-{}", Uuid::new_v4()));
        fs::create_dir_all(&staged_root)?;
        let staged_entries =
            match stage_entry_directories(&self.entries_dir(), &removed_entries, &staged_root) {
                Ok(entries) => entries,
                Err(error) => {
                    if let Some(batch) = recycle_batch {
                        let _ = fs::remove_dir_all(batch);
                    }
                    return Err(error);
                }
            };
        catalog
            .categories
            .retain(|item| !category_ids.contains(&item.id));
        catalog
            .entries
            .retain(|item| !removed_entries.iter().any(|removed| removed.id == item.id));
        if let Err(error) = self.write_catalog(&mut catalog) {
            for (original, temporary) in staged_entries.iter().rev() {
                let _ = fs::rename(temporary, original);
            }
            let _ = fs::remove_dir_all(&staged_root);
            if let Some(batch) = recycle_batch {
                let _ = fs::remove_dir_all(batch);
            }
            return Err(error);
        }
        if staged_root.exists() {
            fs::remove_dir_all(staged_root)?;
        }
        Ok(())
    }

    /// Lists recoverable deletion batches.
    ///
    /// # Errors
    /// Returns an error when recycle manifests or files cannot be read.
    pub fn list_recycle_items(&self, recycle_root: &Path) -> Result<Vec<RecycleItem>> {
        if !recycle_root.exists() {
            return Ok(Vec::new());
        }
        let mut items = Vec::new();
        for item in fs::read_dir(recycle_root)? {
            let path = item?.path();
            let manifest_path = path.join("manifest.json");
            if !manifest_path.is_file() {
                continue;
            }
            let manifest: RecycleManifest = read_json(&manifest_path)?;
            items.push(RecycleItem {
                id: manifest.id,
                kind: manifest.kind,
                display_name: manifest.display_name,
                deleted_at_ms: manifest.deleted_at_ms,
                size_bytes: directory_size(&path)?,
                entry_count: manifest.entries.len(),
            });
        }
        items.sort_by_key(|item| Reverse(item.deleted_at_ms));
        Ok(items)
    }

    /// Restores one deletion batch without overwriting live identifiers or sibling names.
    ///
    /// # Errors
    /// Returns an error for conflicts, failed verification, or failed writes.
    pub fn restore_recycle_item(&self, recycle_root: &Path, item_id: &str) -> Result<()> {
        let batch = recycle_root.join(item_id);
        let manifest: RecycleManifest = read_json(&batch.join("manifest.json"))?;
        let mut catalog = self.read_catalog()?;
        for entry in &manifest.entries {
            if catalog
                .entries
                .iter()
                .any(|current| current.id == entry.id || current.folder == entry.folder)
                || self.entries_dir().join(&entry.folder).exists()
            {
                return Err(StorageError::RestoreConflict(format!(
                    "存档“{}”与现有内容冲突",
                    entry.name
                )));
            }
        }
        for category in &manifest.categories {
            if catalog.categories.iter().any(|current| {
                current.id == category.id
                    || (current.parent_id == category.parent_id
                        && current.name.eq_ignore_ascii_case(&category.name))
            }) {
                return Err(StorageError::RestoreConflict(format!(
                    "分类“{}”与现有内容冲突",
                    category.name
                )));
            }
            if let Some(parent_id) = &category.parent_id
                && !catalog
                    .categories
                    .iter()
                    .any(|current| &current.id == parent_id)
                && !manifest
                    .categories
                    .iter()
                    .any(|current| &current.id == parent_id)
            {
                return Err(StorageError::RestoreConflict(format!(
                    "分类“{}”的上级分类不存在",
                    category.name
                )));
            }
        }
        let mut restored_folders = Vec::new();
        for entry in &manifest.entries {
            let destination = self.entries_dir().join(&entry.folder);
            if let Err(error) = copy_directory_verified(&batch.join(&entry.folder), &destination) {
                for folder in restored_folders {
                    let _ = fs::remove_dir_all(folder);
                }
                return Err(error);
            }
            restored_folders.push(destination);
        }
        catalog.categories.extend(manifest.categories.clone());
        catalog.entries.extend(manifest.entries.clone());
        if let Err(error) = self.write_catalog(&mut catalog) {
            for folder in restored_folders {
                let _ = fs::remove_dir_all(folder);
            }
            return Err(error);
        }
        fs::remove_dir_all(batch)?;
        Ok(())
    }

    /// Permanently removes one recycle bundle.
    ///
    /// # Errors
    /// Returns an error when the bundle cannot be removed.
    pub fn permanently_delete_recycle_item(
        &self,
        recycle_root: &Path,
        item_id: &str,
    ) -> Result<()> {
        let target = recycle_root.join(item_id);
        if target.is_dir() {
            fs::remove_dir_all(target)?;
        }
        Ok(())
    }

    /// Permanently removes all recycle bundles.
    ///
    /// # Errors
    /// Returns an error when a bundle cannot be enumerated or removed.
    pub fn empty_recycle_bin(&self, recycle_root: &Path) -> Result<()> {
        if recycle_root.exists() {
            for item in fs::read_dir(recycle_root)? {
                let path = item?.path();
                if path.is_dir() {
                    fs::remove_dir_all(path)?;
                }
            }
        }
        Ok(())
    }

    /// Lists all registered entries.
    ///
    /// # Errors
    /// Returns an error when catalog metadata cannot be read.
    pub fn list_entries(&self) -> Result<Vec<Entry>> {
        let catalog = self.read_catalog()?;
        let mut entries = catalog
            .entries
            .iter()
            .map(|item| self.get_entry(&item.id))
            .collect::<Result<Vec<Entry>>>()?;
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(entries)
    }

    /// Loads one registered entry.
    ///
    /// # Errors
    /// Returns an error when the entry does not exist or is invalid.
    pub fn get_entry(&self, entry_id: &str) -> Result<Entry> {
        let summary = self
            .read_catalog()?
            .entries
            .into_iter()
            .find(|item| item.id == entry_id)
            .ok_or_else(|| StorageError::EntryNotFound(entry_id.into()))?;
        let mut entry = Entry {
            id: summary.id,
            name: summary.name,
            sources: summary.sources,
            category_id: summary.category_id,
            tags: summary.tags,
            storage_policy: summary.storage_policy,
            sync_mode: summary.sync_mode,
            created_at_ms: summary.created_at_ms,
        };
        let bindings = self.read_bindings()?;
        if let Some(bound) = bindings.entries.get(entry_id) {
            for source in &mut entry.sources {
                if let Some(binding) = bound.iter().find(|binding| binding.id == source.id) {
                    source.path.clone_from(&binding.path);
                }
            }
        }
        Ok(entry)
    }

    /// Returns the directory containing this entry's immutable snapshot archives.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested entry does not exist.
    pub fn entry_storage_path(&self, entry_id: &str) -> Result<PathBuf> {
        self.entry_dir(entry_id)
    }

    /// Lists all category nodes stored in the repository catalog.
    ///
    /// # Errors
    /// Returns an error when catalog metadata cannot be read.
    pub fn list_categories(&self) -> Result<Vec<Category>> {
        Ok(self.read_catalog()?.categories)
    }

    /// Creates a root category or a child of an existing category.
    ///
    /// # Errors
    /// Returns an error for an empty name, missing parent, duplicate sibling, or write failure.
    pub fn create_category(&self, name: &str, parent_id: Option<String>) -> Result<Category> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::EmptyCategoryName);
        }
        let mut catalog = self.read_catalog()?;
        if let Some(parent) = &parent_id
            && !catalog
                .categories
                .iter()
                .any(|category| &category.id == parent)
        {
            return Err(StorageError::ParentCategoryNotFound(parent.clone()));
        }
        if catalog.categories.iter().any(|category| {
            category.parent_id == parent_id && category.name.eq_ignore_ascii_case(name)
        }) {
            return Err(StorageError::DuplicateCategory);
        }
        let category = Category {
            id: Uuid::new_v4().to_string(),
            name: name.to_owned(),
            parent_id,
        };
        catalog.categories.push(category.clone());
        self.write_catalog(&mut catalog)?;
        Ok(category)
    }

    /// Moves a category below another category or back to the root.
    ///
    /// # Errors
    /// Returns an error when either category is missing, the move creates a cycle,
    /// a sibling has the same name, or catalog metadata cannot be written.
    pub fn move_category(&self, category_id: &str, parent_id: Option<&str>) -> Result<()> {
        if parent_id == Some(category_id) {
            return Err(StorageError::InvalidCategoryMove);
        }
        let mut catalog = self.read_catalog()?;
        let category = catalog
            .categories
            .iter()
            .find(|item| item.id == category_id)
            .cloned()
            .ok_or_else(|| StorageError::ParentCategoryNotFound(category_id.into()))?;
        if let Some(parent_id) = parent_id
            && !catalog.categories.iter().any(|item| item.id == parent_id)
        {
            return Err(StorageError::ParentCategoryNotFound(parent_id.into()));
        }
        let mut ancestor = parent_id;
        while let Some(current) = ancestor {
            if current == category_id {
                return Err(StorageError::InvalidCategoryMove);
            }
            ancestor = catalog
                .categories
                .iter()
                .find(|item| item.id == current)
                .and_then(|item| item.parent_id.as_deref());
        }
        if catalog.categories.iter().any(|item| {
            item.id != category_id
                && item.parent_id.as_deref() == parent_id
                && item.name.eq_ignore_ascii_case(&category.name)
        }) {
            return Err(StorageError::DuplicateCategory);
        }
        let target = catalog
            .categories
            .iter_mut()
            .find(|item| item.id == category_id)
            .ok_or_else(|| StorageError::ParentCategoryNotFound(category_id.into()))?;
        target.parent_id = parent_id.map(str::to_owned);
        self.write_catalog(&mut catalog)
    }

    /// Moves an entry into a local category.
    ///
    /// # Errors
    /// Returns an error when the entry does not exist or metadata cannot be written.
    pub fn set_entry_category(&self, entry_id: &str, category_id: Option<String>) -> Result<()> {
        let mut catalog = self.read_catalog()?;
        if let Some(category) = &category_id
            && !catalog.categories.iter().any(|item| &item.id == category)
        {
            return Err(StorageError::ParentCategoryNotFound(category.clone()));
        }
        let summary = catalog
            .entries
            .iter_mut()
            .find(|item| item.id == entry_id)
            .ok_or_else(|| StorageError::EntryNotFound(entry_id.into()))?;
        summary.category_id = category_id;
        self.write_catalog(&mut catalog)
    }

    /// Replaces the searchable tags attached to an entry.
    ///
    /// Empty and duplicate values are removed before metadata is written.
    ///
    /// # Errors
    /// Returns an error when the entry does not exist or metadata cannot be written.
    pub fn set_entry_tags(&self, entry_id: &str, tags: Vec<String>) -> Result<()> {
        let mut normalized = Vec::<String>::new();
        for tag in tags {
            let tag = tag.trim();
            if !tag.is_empty() && !normalized.iter().any(|item| item.eq_ignore_ascii_case(tag)) {
                normalized.push(tag.to_owned());
            }
        }
        let mut catalog = self.read_catalog()?;
        let summary = catalog
            .entries
            .iter_mut()
            .find(|item| item.id == entry_id)
            .ok_or_else(|| StorageError::EntryNotFound(entry_id.into()))?;
        summary.tags = normalized;
        self.write_catalog(&mut catalog)
    }

    /// Lists an entry's snapshots from newest to oldest.
    ///
    /// # Errors
    /// Returns an error when timeline metadata cannot be read.
    pub fn list_snapshots(&self, entry_id: &str) -> Result<Vec<Snapshot>> {
        let mut snapshots = self
            .read_catalog()?
            .entries
            .into_iter()
            .find(|entry| entry.id == entry_id)
            .ok_or_else(|| StorageError::EntryNotFound(entry_id.into()))?
            .snapshots;
        snapshots.sort_by_key(|snapshot| Reverse(snapshot.created_at_ms));
        Ok(snapshots)
    }

    /// Loads one snapshot manifest.
    ///
    /// # Errors
    /// Returns an error when the snapshot does not exist.
    pub fn get_snapshot(&self, snapshot_id: &str) -> Result<Snapshot> {
        for entry in self.read_catalog()?.entries {
            if let Some(snapshot) = entry
                .snapshots
                .into_iter()
                .find(|snapshot| snapshot.id == snapshot_id)
            {
                return Ok(snapshot);
            }
        }
        Err(StorageError::SnapshotNotFound(snapshot_id.into()))
    }

    /// Captures every source in an entry into one immutable 7z archive.
    ///
    /// # Errors
    /// Returns an error when sources cannot be scanned, archived, or committed.
    pub fn create_snapshot(
        &self,
        entry_id: &str,
        title: impl Into<String>,
        device_id: impl Into<String>,
        safety: bool,
    ) -> Result<Snapshot> {
        let entry = self.get_entry(entry_id)?;
        let created_at_ms = unix_millis(SystemTime::now());
        let snapshot_id = Uuid::new_v4().to_string();
        let archive_name = snapshot_archive_name(created_at_ms, &snapshot_id);
        let working = self.temp_dir().join(format!("capture-{snapshot_id}"));
        let content = working.join("content");
        fs::create_dir_all(&content)?;
        let files = stage_sources(&entry.sources, &content)?;
        let mut timeline = self.list_snapshots(entry_id)?;
        let parent_id = timeline.first().map(|snapshot| snapshot.id.clone());
        let changes = compare_manifests(
            timeline.first().map(|snapshot| snapshot.files.as_slice()),
            &files,
        );
        let entry_dir = self.entry_dir(entry_id)?;
        let temporary_archive = self.temp_dir().join(format!("{snapshot_id}.7z"));
        compress_to_path(&content, &temporary_archive)?;
        let object_hash = hash_file(&temporary_archive)?;
        let final_archive = entry_dir.join(&archive_name);
        fs::rename(&temporary_archive, &final_archive)?;
        if working.exists() {
            fs::remove_dir_all(&working)?;
        }
        let snapshot = Snapshot {
            id: snapshot_id,
            entry_id: entry.id,
            parent_id,
            device_id: device_id.into(),
            device_name: self.device_identity()?.1,
            title: title.into(),
            note: String::new(),
            created_at_ms,
            archive_name,
            object_hash,
            size_bytes: final_archive.metadata()?.len(),
            files,
            changes,
            safety,
        };
        timeline.push(snapshot.clone());
        timeline.sort_by_key(|item| item.created_at_ms);
        self.refresh_catalog_entry(entry_id, &timeline)?;
        Ok(snapshot)
    }

    /// Updates the shared user note for one snapshot.
    ///
    /// # Errors
    /// Returns an error when the entry or snapshot does not exist.
    pub fn update_snapshot_note(
        &self,
        entry_id: &str,
        snapshot_id: &str,
        note: impl Into<String>,
    ) -> Result<Snapshot> {
        let mut timeline = self.list_snapshots(entry_id)?;
        let snapshot = timeline
            .iter_mut()
            .find(|snapshot| snapshot.id == snapshot_id)
            .ok_or_else(|| StorageError::SnapshotNotFound(snapshot_id.into()))?;
        note.into().trim().clone_into(&mut snapshot.note);
        let updated = snapshot.clone();
        self.refresh_catalog_entry(entry_id, &timeline)?;
        Ok(updated)
    }

    /// Permanently deletes one stored snapshot and its archive object.
    ///
    /// # Errors
    /// Returns an error when the entry or snapshot does not exist, or its archive cannot be removed.
    pub fn delete_snapshot(&self, entry_id: &str, snapshot_id: &str) -> Result<()> {
        let timeline = self.list_snapshots(entry_id)?;
        let snapshot = timeline
            .iter()
            .find(|snapshot| snapshot.id == snapshot_id)
            .ok_or_else(|| StorageError::SnapshotNotFound(snapshot_id.into()))?;
        let archive = self.entry_dir(entry_id)?.join(&snapshot.archive_name);
        let staged = self.temp_dir().join(format!("delete-{snapshot_id}.7z"));
        fs::rename(&archive, &staged)?;
        let remaining = timeline
            .into_iter()
            .filter(|item| item.id != snapshot_id)
            .collect::<Vec<_>>();
        if let Err(error) = self.refresh_catalog_entry(entry_id, &remaining) {
            let _ = fs::rename(&staged, &archive);
            return Err(error);
        }
        fs::remove_file(staged)?;
        Ok(())
    }

    /// Verifies a snapshot archive against its SHA-256 hash.
    ///
    /// # Errors
    /// Returns an error when metadata or the archive cannot be read.
    pub fn verify_snapshot(&self, snapshot_id: &str) -> Result<bool> {
        let snapshot = self.get_snapshot(snapshot_id)?;
        let archive = self
            .entry_dir(&snapshot.entry_id)?
            .join(&snapshot.archive_name);
        Ok(archive.exists() && hash_file(&archive)? == snapshot.object_hash)
    }

    /// Restores every source after capturing a safety snapshot.
    ///
    /// # Errors
    /// Returns an error when verification, extraction, or replacement fails.
    pub fn restore_snapshot(&self, entry_id: &str, snapshot_id: &str) -> Result<()> {
        let entry = self.get_entry(entry_id)?;
        let snapshot = self.get_snapshot(snapshot_id)?;
        if snapshot.entry_id != entry.id {
            return Err(StorageError::SnapshotNotFound(snapshot_id.into()));
        }
        self.create_snapshot(entry_id, "恢复前安全快照", "local", true)?;
        if !self.verify_snapshot(snapshot_id)? {
            return Err(StorageError::IntegrityMismatch);
        }
        let restore_root = self.temp_dir().join(format!("restore-{}", Uuid::new_v4()));
        decompress_file(
            self.entry_dir(entry_id)?.join(&snapshot.archive_name),
            &restore_root,
        )?;
        for source in &entry.sources {
            let extracted_root = restore_root.join(&source.id);
            let target = PathBuf::from(&source.path);
            match source.kind {
                EntryKind::Directory => {
                    if !extracted_root.is_dir() {
                        return Err(StorageError::IncompleteSnapshot(source.id.clone()));
                    }
                    replace_path(&target, &extracted_root, true)?;
                }
                EntryKind::File => {
                    let extracted = extracted_root.join(&source.name);
                    if !extracted.is_file() {
                        return Err(StorageError::IncompleteSnapshot(source.id.clone()));
                    }
                    replace_path(&target, &extracted, false)?;
                }
            }
        }
        if restore_root.exists() {
            fs::remove_dir_all(restore_root)?;
        }
        Ok(())
    }

    fn initialize(&self) -> Result<()> {
        for directory in [self.config_dir(), self.entries_dir(), self.temp_dir()] {
            fs::create_dir_all(directory)?;
        }
        if !self.settings_path().exists() {
            write_json_atomic(
                &self.settings_path(),
                &serde_json::json!({
                    "formatVersion": SETTINGS_VERSION, "app": {}, "cloud": {}
                }),
                &self.temp_dir(),
            )?;
        }
        if !self.catalog_path().exists() {
            write_json_atomic(
                &self.catalog_path(),
                &Catalog {
                    format_version: FORMAT_VERSION,
                    updated_at_ms: unix_millis(SystemTime::now()),
                    categories: Vec::new(),
                    entries: Vec::new(),
                },
                &self.temp_dir(),
            )?;
        }
        if !self.bindings_path().exists() {
            write_json_atomic(
                &self.bindings_path(),
                &BindingsDocument {
                    format_version: 1,
                    entries: HashMap::new(),
                },
                &self.temp_dir(),
            )?;
        }
        if !self.device_path().exists() {
            write_json_atomic(
                &self.device_path(),
                &DeviceDocument {
                    format_version: 1,
                    id: Uuid::new_v4().to_string(),
                    name: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "本地设备".into()),
                },
                &self.temp_dir(),
            )?;
        }
        if !self.library_path().exists() {
            write_json_atomic(
                &self.library_path(),
                &serde_json::json!({
                    "formatVersion": 2,
                    "libraryId": Uuid::new_v4().to_string(),
                    "updatedAtMs": unix_millis(SystemTime::now())
                }),
                &self.temp_dir(),
            )?;
        }
        self.migrate_source_bindings()?;
        Ok(())
    }

    fn config_dir(&self) -> PathBuf {
        self.root.join("config")
    }
    fn settings_path(&self) -> PathBuf {
        self.config_dir().join("settings.json")
    }
    fn bindings_path(&self) -> PathBuf {
        self.config_dir().join("bindings.json")
    }
    fn device_path(&self) -> PathBuf {
        self.config_dir().join("device.json")
    }
    fn library_path(&self) -> PathBuf {
        self.root.join("library.json")
    }
    fn catalog_path(&self) -> PathBuf {
        self.root.join("catalog.json")
    }
    fn entries_dir(&self) -> PathBuf {
        self.root.join("archives")
    }
    fn temp_dir(&self) -> PathBuf {
        self.root.join(".tmp")
    }
    fn read_catalog(&self) -> Result<Catalog> {
        read_json(&self.catalog_path())
    }
    fn write_catalog(&self, catalog: &mut Catalog) -> Result<()> {
        catalog.updated_at_ms = unix_millis(SystemTime::now());
        write_json_atomic(&self.catalog_path(), catalog, &self.temp_dir())
    }
    fn entry_dir(&self, entry_id: &str) -> Result<PathBuf> {
        self.read_catalog()?
            .entries
            .into_iter()
            .find(|entry| entry.id == entry_id)
            .map(|entry| self.entries_dir().join(entry.folder))
            .ok_or_else(|| StorageError::EntryNotFound(entry_id.into()))
    }
    fn refresh_catalog_entry(&self, entry_id: &str, timeline: &[Snapshot]) -> Result<()> {
        let mut catalog = self.read_catalog()?;
        let summary = catalog
            .entries
            .iter_mut()
            .find(|entry| entry.id == entry_id)
            .ok_or_else(|| StorageError::EntryNotFound(entry_id.into()))?;
        summary.snapshot_count = timeline.len();
        summary.stored_bytes = timeline.iter().map(|snapshot| snapshot.size_bytes).sum();
        summary.last_snapshot_at_ms = timeline.last().map(|snapshot| snapshot.created_at_ms);
        summary.snapshots = timeline.to_vec();
        self.write_catalog(&mut catalog)
    }

    /// Returns the stable local device identifier and display name.
    ///
    /// # Errors
    /// Returns an error when the device document cannot be read.
    pub fn device_identity(&self) -> Result<(String, String)> {
        let device: DeviceDocument = read_json(&self.device_path())?;
        Ok((device.id, device.name))
    }

    fn read_bindings(&self) -> Result<BindingsDocument> {
        read_json(&self.bindings_path())
    }

    fn save_bindings_for_entry(&self, entry_id: &str, sources: &[EntrySource]) -> Result<()> {
        let mut bindings = self.read_bindings()?;
        let saved = bindings.entries.entry(entry_id.to_owned()).or_default();
        for source in sources {
            if let Some(existing) = saved.iter_mut().find(|existing| existing.id == source.id) {
                *existing = source.clone();
            } else {
                saved.push(source.clone());
            }
        }
        write_json_atomic(&self.bindings_path(), &bindings, &self.temp_dir())
    }

    fn migrate_source_bindings(&self) -> Result<()> {
        let mut catalog = self.read_catalog()?;
        let mut bindings = self.read_bindings()?;
        let mut bindings_changed = false;
        let legacy_entries = self.root.join("data").join("entries");
        for summary in &mut catalog.entries {
            let legacy = legacy_entries.join(&summary.folder);
            if summary.sources.is_empty() && legacy.join("entry.json").is_file() {
                let entry: Entry = read_json(&legacy.join("entry.json"))?;
                summary.sources = shared_entry(&entry).sources;
                summary.created_at_ms = entry.created_at_ms;
                summary.category_id = entry.category_id;
                summary.tags = entry.tags;
                summary.storage_policy = entry.storage_policy;
                summary.sync_mode = entry.sync_mode;
                let saved = bindings.entries.entry(entry.id.clone()).or_default();
                for source in &entry.sources {
                    if !source.path.is_empty()
                        && !saved.iter().any(|existing| existing.id == source.id)
                    {
                        saved.push(source.clone());
                    }
                }
                bindings_changed = true;
            }
            if summary.snapshots.is_empty() && legacy.join("timeline.json").is_file() {
                summary.snapshots = read_json(&legacy.join("timeline.json"))?;
                summary.snapshot_count = summary.snapshots.len();
                summary.stored_bytes = summary
                    .snapshots
                    .iter()
                    .map(|snapshot| snapshot.size_bytes)
                    .sum();
                summary.last_snapshot_at_ms = summary
                    .snapshots
                    .last()
                    .map(|snapshot| snapshot.created_at_ms);
            }
            let destination = self.entries_dir().join(&summary.folder);
            if legacy.is_dir() && !destination.exists() {
                fs::create_dir_all(&destination)?;
                for snapshot in &summary.snapshots {
                    let source = legacy.join(&snapshot.archive_name);
                    if source.is_file() {
                        fs::copy(source, destination.join(&snapshot.archive_name))?;
                    }
                }
            }
        }
        if bindings_changed {
            write_json_atomic(&self.bindings_path(), &bindings, &self.temp_dir())?;
        }
        if catalog.format_version < FORMAT_VERSION {
            catalog.format_version = FORMAT_VERSION;
            self.write_catalog(&mut catalog)?;
        }
        Ok(())
    }
}

fn shared_entry(entry: &Entry) -> Entry {
    let mut shared = entry.clone();
    shared
        .sources
        .iter_mut()
        .for_each(|source| source.path.clear());
    shared
}

fn resolve_entry_sources(
    source_paths: &[String],
    existing: &[EntrySource],
) -> Result<Vec<EntrySource>> {
    let mut sources = Vec::with_capacity(source_paths.len());
    for source_path in source_paths {
        let path = Path::new(source_path);
        if !path.exists() {
            return Err(StorageError::MissingSource(path.to_path_buf()));
        }
        let canonical = path.canonicalize()?;
        let metadata = canonical.metadata()?;
        let source_name = canonical
            .file_name()
            .ok_or_else(|| StorageError::MissingName(canonical.clone()))?
            .to_string_lossy()
            .into_owned();
        let path = path_string(&canonical)?;
        sources.push(EntrySource {
            id: existing
                .iter()
                .find(|source| source.path.eq_ignore_ascii_case(&path))
                .map_or_else(|| Uuid::new_v4().to_string(), |source| source.id.clone()),
            name: source_name,
            path,
            kind: if metadata.is_dir() {
                EntryKind::Directory
            } else {
                EntryKind::File
            },
        });
    }
    Ok(sources)
}

fn resolve_entry_sources_by_position(
    source_paths: &[String],
    existing: &[EntrySource],
) -> Result<Vec<EntrySource>> {
    let mut resolved = resolve_entry_sources(source_paths, existing)?;
    for (index, source) in resolved.iter_mut().enumerate() {
        if let Some(previous) = existing.get(index) {
            source.id.clone_from(&previous.id);
        }
    }
    Ok(resolved)
}

fn recycle_batch(root: &Path, kind: &str, id: &str) -> Result<PathBuf> {
    fs::create_dir_all(root)?;
    let batch = root.join(format!(
        "{}__{}__{}__{}",
        unix_millis(SystemTime::now()),
        kind,
        safe_folder_name(id),
        Uuid::new_v4()
    ));
    fs::create_dir_all(&batch)?;
    Ok(batch)
}

fn stage_entry_directories(
    entries_dir: &Path,
    entries: &[CatalogEntry],
    staged_root: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    let mut staged_entries = Vec::new();
    for entry in entries {
        let source = entries_dir.join(&entry.folder);
        if !source.exists() {
            continue;
        }
        let staged = staged_root.join(&entry.folder);
        if let Err(error) = fs::rename(&source, &staged) {
            for (original, temporary) in staged_entries.iter().rev() {
                let _ = fs::rename(temporary, original);
            }
            let _ = fs::remove_dir_all(staged_root);
            return Err(error.into());
        }
        staged_entries.push((source, staged));
    }
    Ok(staged_entries)
}

fn copy_directory_verified(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for item in WalkDir::new(source).min_depth(1) {
        let item = item?;
        let relative = item
            .path()
            .strip_prefix(source)
            .map_err(|error| io::Error::other(error.to_string()))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(relative);
        if item.file_type().is_dir() {
            fs::create_dir_all(target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(item.path(), target)?;
            let copied = destination.join(relative);
            if item.metadata()?.len() != copied.metadata()?.len()
                || hash_file(item.path())? != hash_file(&copied)?
            {
                return Err(StorageError::IntegrityMismatch);
            }
        }
    }
    Ok(())
}

fn directory_size(path: &Path) -> Result<u64> {
    let mut total = 0_u64;
    for item in WalkDir::new(path) {
        let item = item?;
        if item.file_type().is_file() {
            total = total.saturating_add(item.metadata()?.len());
        }
    }
    Ok(total)
}

fn stage_sources(sources: &[EntrySource], destination: &Path) -> Result<Vec<SnapshotFile>> {
    let mut files = Vec::new();
    for source in sources {
        let path = Path::new(&source.path);
        if !path.exists() {
            return Err(StorageError::MissingSource(path.to_path_buf()));
        }
        let source_root = destination.join(&source.id);
        match source.kind {
            EntryKind::File => {
                fs::create_dir_all(&source_root)?;
                fs::copy(path, source_root.join(&source.name))?;
                files.push(snapshot_file(
                    path,
                    &Path::new(&source.id).join(&source.name),
                )?);
            }
            EntryKind::Directory => {
                fs::create_dir_all(&source_root)?;
                for item in WalkDir::new(path).min_depth(1) {
                    let item = item?;
                    let relative = item
                        .path()
                        .strip_prefix(path)
                        .expect("walked path must be below source");
                    let staged = source_root.join(relative);
                    if item.file_type().is_dir() {
                        fs::create_dir_all(staged)?;
                    } else if item.file_type().is_file() {
                        if let Some(parent) = staged.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        fs::copy(item.path(), &staged)?;
                        files.push(snapshot_file(
                            item.path(),
                            &Path::new(&source.id).join(relative),
                        )?);
                    }
                }
            }
        }
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(files)
}

fn snapshot_file(source: &Path, archived_path: &Path) -> Result<SnapshotFile> {
    let metadata = source.metadata()?;
    Ok(SnapshotFile {
        relative_path: archive_path(archived_path)?,
        content_hash: hash_file(source)?,
        size_bytes: metadata.len(),
        modified_at_ms: metadata.modified().map(unix_millis).unwrap_or_default(),
    })
}

fn compare_manifests(previous: Option<&[SnapshotFile]>, current: &[SnapshotFile]) -> ChangeSummary {
    let previous = previous.unwrap_or_default();
    let old: HashMap<&str, &str> = previous
        .iter()
        .map(|file| (file.relative_path.as_str(), file.content_hash.as_str()))
        .collect();
    let new: HashMap<&str, &str> = current
        .iter()
        .map(|file| (file.relative_path.as_str(), file.content_hash.as_str()))
        .collect();
    ChangeSummary {
        added: new.keys().filter(|path| !old.contains_key(**path)).count() as u64,
        modified: new
            .iter()
            .filter(|(path, hash)| old.get(**path).is_some_and(|old_hash| old_hash != *hash))
            .count() as u64,
        deleted: old.keys().filter(|path| !new.contains_key(**path)).count() as u64,
    }
}

fn snapshot_archive_name(created_at_ms: u64, snapshot_id: &str) -> String {
    let seconds = i64::try_from(created_at_ms / 1_000).unwrap_or(i64::MAX);
    let time = OffsetDateTime::from_unix_timestamp(seconds).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    let id = snapshot_id.get(..8).unwrap_or(snapshot_id);
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}.{:03}Z_{id}.7z",
        time.year(),
        u8::from(time.month()),
        time.day(),
        time.hour(),
        time.minute(),
        time.second(),
        created_at_ms % 1_000
    )
}

fn safe_folder_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect();
    let sanitized = sanitized.trim().trim_end_matches(['.', ' ']);
    if sanitized.is_empty() {
        "archive".into()
    } else {
        sanitized.chars().take(80).collect()
    }
}

fn replace_path(target: &Path, replacement: &Path, directory: bool) -> Result<()> {
    let old = target.with_file_name(format!(".chronicle-old-{}", Uuid::new_v4()));
    let staged = target.with_file_name(format!(".chronicle-new-{}", Uuid::new_v4()));
    if directory {
        copy_directory(replacement, &staged)?;
    } else {
        fs::copy(replacement, &staged)?;
    }
    if target.exists() {
        fs::rename(target, &old)?;
    }
    if let Err(error) = fs::rename(&staged, target) {
        if old.exists() {
            fs::rename(&old, target)?;
        }
        return Err(error.into());
    }
    if old.exists() {
        if directory {
            fs::remove_dir_all(old)?;
        } else {
            fs::remove_file(old)?;
        }
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for item in WalkDir::new(source).min_depth(1) {
        let item = item?;
        let relative = item
            .path()
            .strip_prefix(source)
            .expect("walked path must be below source");
        let target = destination.join(relative);
        if item.file_type().is_dir() {
            fs::create_dir_all(target)?;
        } else if item.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(item.path(), target)?;
        }
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(encoded)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T, temp_dir: &Path) -> Result<()> {
    fs::create_dir_all(temp_dir)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = temp_dir.join(format!("{}.json", Uuid::new_v4()));
    let mut writer = BufWriter::new(File::create(&temporary)?);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);
    let backup = temp_dir.join(format!("{}.backup", Uuid::new_v4()));
    if path.exists() {
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() {
            fs::rename(&backup, path)?;
        }
        return Err(error.into());
    }
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    Ok(())
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| StorageError::NonUtf8Path(path.into()))
}

fn archive_path(path: &Path) -> Result<String> {
    path.components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| StorageError::NonUtf8Path(path.into()))
        })
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join("/"))
}

fn unix_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::LocalRepository;
    use chronicle_core::{StoragePolicy, SyncMode};
    use std::{fs, io::Read, path::Path};
    use tempfile::tempdir;

    #[test]
    fn multi_source_snapshots_use_sevenz_timeline_and_restore() {
        let workspace = tempdir().unwrap();
        let folder = workspace.path().join("folder");
        let file = workspace.path().join("settings.json");
        fs::create_dir_all(folder.join("nested")).unwrap();
        fs::write(folder.join("nested/note.txt"), b"keep").unwrap();
        fs::write(&file, b"version one").unwrap();
        let repository_root = workspace.path().join("Chronicle");
        let repository = LocalRepository::open(&repository_root).unwrap();
        assert_eq!(repository.load_settings().unwrap()["formatVersion"], 3);
        assert!(
            repository
                .save_settings(&serde_json::json!({ "app": {}, "cloud": {} }))
                .is_err()
        );
        repository
            .save_settings(&serde_json::json!({
                "formatVersion": 3,
                "app": { "defaultCategory": "配置" },
                "cloud": { "enabled": false }
            }))
            .unwrap();
        let entry = repository
            .add_entry_sources(
                "Mixed Save",
                &[
                    folder.to_string_lossy().into_owned(),
                    file.to_string_lossy().into_owned(),
                ],
                Some("配置".into()),
                StoragePolicy::LocalAndRemote,
            )
            .unwrap();
        let category = repository.create_category("游戏", None).unwrap();
        let child = repository
            .create_category("角色扮演", Some(category.id.clone()))
            .unwrap();
        repository
            .set_entry_category(&entry.id, Some(child.id.clone()))
            .unwrap();
        assert_eq!(
            repository.get_entry(&entry.id).unwrap().category_id,
            Some(child.id)
        );
        assert_eq!(repository.list_categories().unwrap().len(), 2);
        let first = repository
            .create_snapshot(&entry.id, "初始版本", "test", false)
            .unwrap();
        assert!(
            Path::new(&first.archive_name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("7z"))
        );
        assert_eq!(first.files.len(), 2);
        let entry_folder = fs::read_dir(repository_root.join("archives"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let mut signature = [0_u8; 6];
        fs::File::open(entry_folder.join(&first.archive_name))
            .unwrap()
            .read_exact(&mut signature)
            .unwrap();
        assert_eq!(signature, [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]);
        assert!(repository_root.join("config/settings.json").is_file());
        assert!(repository_root.join("config/device.json").is_file());
        assert!(repository_root.join("config/bindings.json").is_file());
        assert!(repository_root.join("library.json").is_file());
        assert!(repository_root.join("catalog.json").is_file());
        let shared_entry: serde_json::Value =
            super::read_json(&repository_root.join("catalog.json")).unwrap();
        assert!(
            shared_entry["entries"][0]["sources"]
                .as_array()
                .unwrap()
                .iter()
                .all(|source| source.get("path").is_none())
        );
        fs::write(folder.join("nested/note.txt"), b"changed").unwrap();
        fs::write(&file, b"version two").unwrap();
        let second = repository
            .create_snapshot(&entry.id, "修改版本", "test", false)
            .unwrap();
        assert_eq!(second.changes.modified, 2);
        assert!(repository.verify_snapshot(&first.id).unwrap());
        repository.restore_snapshot(&entry.id, &first.id).unwrap();
        assert_eq!(fs::read(folder.join("nested/note.txt")).unwrap(), b"keep");
        assert_eq!(fs::read(file).unwrap(), b"version one");
        assert_eq!(repository.list_snapshots(&entry.id).unwrap().len(), 3);
    }

    #[test]
    fn categories_return_to_root_and_tags_are_normalized() {
        let workspace = tempdir().unwrap();
        let source = workspace.path().join("settings.json");
        fs::write(&source, b"settings").unwrap();
        let repository = LocalRepository::open(workspace.path().join("Chronicle")).unwrap();
        let entry = repository
            .add_entry(&source, Some("Settings"), None)
            .unwrap();
        let parent = repository.create_category("配置", None).unwrap();
        let child = repository.create_category("游戏", None).unwrap();

        repository
            .move_category(&child.id, Some(&parent.id))
            .unwrap();
        assert!(
            repository
                .move_category(&parent.id, Some(&child.id))
                .is_err()
        );
        repository.move_category(&child.id, None).unwrap();
        assert_eq!(
            repository
                .list_categories()
                .unwrap()
                .into_iter()
                .find(|item| item.id == child.id)
                .unwrap()
                .parent_id,
            None
        );

        repository
            .set_entry_tags(
                &entry.id,
                vec!["重要".into(), "配置".into(), "重要".into(), "  ".into()],
            )
            .unwrap();
        assert_eq!(
            repository.get_entry(&entry.id).unwrap().tags,
            vec!["重要", "配置"]
        );
    }

    #[test]
    fn recycle_entry_and_category_round_trip() {
        let workspace = tempdir().unwrap();
        let source = workspace.path().join("state.dat");
        fs::write(&source, b"state").unwrap();
        let repository = LocalRepository::open(workspace.path().join("Chronicle")).unwrap();
        let category = repository.create_category("项目", None).unwrap();
        let entry = repository
            .add_entry_sources(
                "状态",
                &[source.to_string_lossy().into_owned()],
                Some(category.id.clone()),
                StoragePolicy::Local,
            )
            .unwrap();
        repository
            .create_snapshot(&entry.id, "初始", "device", false)
            .unwrap();
        let recycle = workspace.path().join("Recycle");
        repository
            .delete_category(&category.id, Some(&recycle))
            .unwrap();
        assert!(repository.list_entries().unwrap().is_empty());
        let items = repository.list_recycle_items(&recycle).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].entry_count, 1);
        assert!(
            repository
                .total_stored_bytes_with_recycle(Some(&recycle))
                .unwrap()
                > repository.total_stored_bytes().unwrap()
        );
        repository
            .restore_recycle_item(&recycle, &items[0].id)
            .unwrap();
        assert_eq!(repository.list_categories().unwrap().len(), 1);
        assert_eq!(
            Path::new(&repository.list_entries().unwrap()[0].sources[0].path)
                .canonicalize()
                .unwrap(),
            source.canonicalize().unwrap(),
        );
        assert!(repository.list_recycle_items(&recycle).unwrap().is_empty());
    }

    #[test]
    fn replacing_a_source_keeps_its_stable_slot_id() {
        let workspace = tempdir().unwrap();
        let first = workspace.path().join("first.dat");
        let second = workspace.path().join("second.dat");
        fs::write(&first, b"one").unwrap();
        fs::write(&second, b"two").unwrap();
        let repository = LocalRepository::open(workspace.path().join("Chronicle")).unwrap();
        let entry = repository.add_entry(&first, Some("slot"), None).unwrap();
        let original_id = entry.sources[0].id.clone();
        let updated = repository
            .update_entry_with_sync(
                &entry.id,
                "slot",
                &[second.to_string_lossy().into_owned()],
                StoragePolicy::Local,
                SyncMode::Automatic,
            )
            .unwrap();
        assert_eq!(updated.sources[0].id, original_id);
        assert_eq!(updated.sync_mode, SyncMode::Automatic);
    }

    #[test]
    fn changing_sync_mode_keeps_sources_and_snapshots_intact() {
        let workspace = tempdir().unwrap();
        let source = workspace.path().join("save.dat");
        fs::write(&source, b"save").unwrap();
        let repository = LocalRepository::open(workspace.path().join("Chronicle")).unwrap();
        let entry = repository.add_entry(&source, Some("Save"), None).unwrap();
        repository
            .create_snapshot(&entry.id, "initial", "test", false)
            .unwrap();

        let updated = repository
            .set_entry_sync_mode(&entry.id, SyncMode::Automatic)
            .unwrap();

        assert_eq!(updated.sync_mode, SyncMode::Automatic);
        assert_eq!(updated.sources, entry.sources);
        assert_eq!(repository.list_snapshots(&entry.id).unwrap().len(), 1);
    }

    #[test]
    fn snapshot_notes_persist_and_deleting_a_snapshot_removes_its_archive() {
        let workspace = tempdir().unwrap();
        let source = workspace.path().join("save.dat");
        fs::write(&source, b"save").unwrap();
        let repository = LocalRepository::open(workspace.path().join("Chronicle")).unwrap();
        let entry = repository.add_entry(&source, Some("Save"), None).unwrap();
        let snapshot = repository
            .create_snapshot(&entry.id, "Boss 前", "test", false)
            .unwrap();

        let updated = repository
            .update_snapshot_note(&entry.id, &snapshot.id, "进入第二阶段前")
            .unwrap();
        assert_eq!(updated.note, "进入第二阶段前");
        assert_eq!(
            repository.list_snapshots(&entry.id).unwrap()[0].note,
            "进入第二阶段前"
        );

        let archive = repository
            .entry_dir(&entry.id)
            .unwrap()
            .join(&snapshot.archive_name);
        assert!(archive.is_file());
        repository.delete_snapshot(&entry.id, &snapshot.id).unwrap();
        assert!(repository.list_snapshots(&entry.id).unwrap().is_empty());
        assert!(!archive.exists());
    }
}
