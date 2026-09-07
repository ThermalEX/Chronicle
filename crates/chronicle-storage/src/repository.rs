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
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sevenz_rust::{compress_to_path, decompress_file};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;
use walkdir::WalkDir;

const FORMAT_VERSION: u32 = 2;
const SETTINGS_VERSION: u32 = 1;
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
    #[error("settings must use formatVersion 1 and contain app and cloud objects")]
    InvalidSettings,
    #[error("category name cannot be empty")]
    EmptyCategoryName,
    #[error("parent category not found: {0}")]
    ParentCategoryNotFound(String),
    #[error("a category with this name already exists at the selected level")]
    DuplicateCategory,
    #[error("a category cannot be moved into itself or one of its descendants")]
    InvalidCategoryMove,
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
    source_count: usize,
    snapshot_count: usize,
    stored_bytes: u64,
    last_snapshot_at_ms: Option<u64>,
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
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::EmptyName);
        }
        if source_paths.is_empty() {
            return Err(StorageError::EmptySources);
        }
        let mut sources = Vec::with_capacity(source_paths.len());
        for source_path in source_paths {
            let path = Path::new(source_path);
            if !path.exists() {
                return Err(StorageError::MissingSource(path.to_path_buf()));
            }
            let path = path.canonicalize()?;
            let metadata = path.metadata()?;
            let source_name = path
                .file_name()
                .ok_or_else(|| StorageError::MissingName(path.clone()))?
                .to_string_lossy()
                .into_owned();
            sources.push(EntrySource {
                id: Uuid::new_v4().to_string(),
                name: source_name,
                path: path_string(&path)?,
                kind: if metadata.is_dir() {
                    EntryKind::Directory
                } else {
                    EntryKind::File
                },
            });
        }
        let id = Uuid::new_v4().to_string();
        let folder = format!("{}__{id}", safe_folder_name(name));
        let entry = Entry {
            id,
            name: name.to_owned(),
            sources,
            category_id,
            tags: Vec::new(),
            storage_policy,
            created_at_ms: unix_millis(SystemTime::now()),
        };
        let directory = self.entries_dir().join(&folder);
        fs::create_dir_all(&directory)?;
        write_json_atomic(&directory.join("entry.json"), &entry, &self.temp_dir())?;
        write_json_atomic(
            &directory.join("timeline.json"),
            &Vec::<Snapshot>::new(),
            &self.temp_dir(),
        )?;
        let mut catalog = self.read_catalog()?;
        catalog.entries.push(CatalogEntry {
            id: entry.id.clone(),
            name: entry.name.clone(),
            folder,
            category_id: entry.category_id.clone(),
            tags: entry.tags.clone(),
            storage_policy,
            source_count: entry.sources.len(),
            snapshot_count: 0,
            stored_bytes: 0,
            last_snapshot_at_ms: None,
        });
        self.write_catalog(&mut catalog)?;
        Ok(entry)
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
            .map(|item| read_json(&self.entries_dir().join(&item.folder).join("entry.json")))
            .collect::<Result<Vec<Entry>>>()?;
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(entries)
    }

    /// Loads one registered entry.
    ///
    /// # Errors
    /// Returns an error when the entry does not exist or is invalid.
    pub fn get_entry(&self, entry_id: &str) -> Result<Entry> {
        read_json(&self.entry_dir(entry_id)?.join("entry.json"))
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
        let entry_directory = self.entry_dir(entry_id)?;
        let mut entry: Entry = read_json(&entry_directory.join("entry.json"))?;
        entry.category_id.clone_from(&category_id);
        write_json_atomic(
            &entry_directory.join("entry.json"),
            &entry,
            &self.temp_dir(),
        )?;
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
        let entry_directory = self.entry_dir(entry_id)?;
        let mut entry: Entry = read_json(&entry_directory.join("entry.json"))?;
        entry.tags.clone_from(&normalized);
        write_json_atomic(
            &entry_directory.join("entry.json"),
            &entry,
            &self.temp_dir(),
        )?;
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
        let mut snapshots: Vec<Snapshot> =
            read_json(&self.entry_dir(entry_id)?.join("timeline.json"))?;
        snapshots.sort_by_key(|snapshot| Reverse(snapshot.created_at_ms));
        Ok(snapshots)
    }

    /// Loads one snapshot manifest.
    ///
    /// # Errors
    /// Returns an error when the snapshot does not exist.
    pub fn get_snapshot(&self, snapshot_id: &str) -> Result<Snapshot> {
        for entry in self.read_catalog()?.entries {
            let timeline: Vec<Snapshot> =
                read_json(&self.entries_dir().join(entry.folder).join("timeline.json"))?;
            if let Some(snapshot) = timeline
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
            title: title.into(),
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
        write_json_atomic(
            &entry_dir.join("timeline.json"),
            &timeline,
            &self.temp_dir(),
        )?;
        self.refresh_catalog_entry(entry_id, &timeline)?;
        Ok(snapshot)
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
        Ok(())
    }

    fn config_dir(&self) -> PathBuf {
        self.root.join("config")
    }
    fn settings_path(&self) -> PathBuf {
        self.config_dir().join("settings.json")
    }
    fn data_dir(&self) -> PathBuf {
        self.root.join("data")
    }
    fn catalog_path(&self) -> PathBuf {
        self.data_dir().join("catalog.json")
    }
    fn entries_dir(&self) -> PathBuf {
        self.data_dir().join("entries")
    }
    fn temp_dir(&self) -> PathBuf {
        self.data_dir().join(".tmp")
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
        self.write_catalog(&mut catalog)
    }
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
    use chronicle_core::StoragePolicy;
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
        assert_eq!(repository.load_settings().unwrap()["formatVersion"], 1);
        assert!(
            repository
                .save_settings(&serde_json::json!({ "app": {}, "cloud": {} }))
                .is_err()
        );
        repository
            .save_settings(&serde_json::json!({
                "formatVersion": 1,
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
        let entry_folder = fs::read_dir(repository_root.join("data/entries"))
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
        assert!(repository_root.join("data/catalog.json").is_file());
        assert!(entry_folder.join("entry.json").is_file());
        assert!(entry_folder.join("timeline.json").is_file());
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
}
