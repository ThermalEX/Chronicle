use std::{
    cmp::Reverse,
    collections::HashMap,
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use chronicle_core::{ChangeSummary, Entry, EntryKind, Snapshot, SnapshotFile};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::object_key;

const FORMAT_VERSION: &str = "v1";
const HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("metadata error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("directory scan error: {0}")]
    Walk(#[from] walkdir::Error),
    #[error("source does not exist: {0}")]
    MissingSource(PathBuf),
    #[error("source path has no file name: {0}")]
    MissingName(PathBuf),
    #[error("path cannot be represented as UTF-8: {0}")]
    NonUtf8Path(PathBuf),
    #[error("entry not found: {0}")]
    EntryNotFound(String),
    #[error("snapshot not found: {0}")]
    SnapshotNotFound(String),
    #[error("snapshot object failed content verification")]
    IntegrityMismatch,
    #[error("snapshot contains an unsafe path")]
    UnsafeArchivePath,
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Filesystem-backed Chronicle repository using versioned JSON manifests and ZIP objects.
#[derive(Clone, Debug)]
pub struct LocalRepository {
    root: PathBuf,
}

impl LocalRepository {
    /// Opens or initializes a repository at `root`.
    ///
    /// # Errors
    ///
    /// Returns an error when the repository directories or format manifest cannot be created.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let repository = Self { root: root.into() };
        repository.initialize()?;
        Ok(repository)
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Registers an existing file or directory as a Chronicle entry.
    ///
    /// # Errors
    ///
    /// Returns an error when the source is missing, cannot be inspected, or its manifest cannot be written.
    pub fn add_entry(
        &self,
        source_path: impl AsRef<Path>,
        name: Option<String>,
        category_id: Option<String>,
    ) -> Result<Entry> {
        let source = source_path.as_ref();
        if !source.exists() {
            return Err(StorageError::MissingSource(source.to_path_buf()));
        }
        let source = source.canonicalize()?;
        let metadata = source.metadata()?;
        let kind = if metadata.is_dir() {
            EntryKind::Directory
        } else {
            EntryKind::File
        };
        let default_name = source
            .file_name()
            .ok_or_else(|| StorageError::MissingName(source.clone()))?
            .to_string_lossy()
            .into_owned();
        let entry = Entry {
            id: Uuid::new_v4().to_string(),
            name: name.unwrap_or(default_name),
            source_path: path_string(&source)?,
            kind,
            category_id,
            created_at_ms: unix_millis(SystemTime::now()),
        };
        write_json_atomic(&self.entry_path(&entry.id), &entry, &self.temp_dir())?;
        Ok(entry)
    }

    /// Lists all registered entries.
    ///
    /// # Errors
    ///
    /// Returns an error when an entry manifest cannot be read or decoded.
    pub fn list_entries(&self) -> Result<Vec<Entry>> {
        let mut entries = read_json_directory(&self.entries_dir())?;
        entries.sort_by(|left: &Entry, right| left.name.cmp(&right.name));
        Ok(entries)
    }

    /// Loads one registered entry.
    ///
    /// # Errors
    ///
    /// Returns an error when the entry does not exist or its manifest is invalid.
    pub fn get_entry(&self, entry_id: &str) -> Result<Entry> {
        read_json(&self.entry_path(entry_id))
            .map_err(|error| map_not_found(error, || StorageError::EntryNotFound(entry_id.into())))
    }

    /// Lists an entry's snapshots from newest to oldest.
    ///
    /// # Errors
    ///
    /// Returns an error when a snapshot manifest cannot be read or decoded.
    pub fn list_snapshots(&self, entry_id: &str) -> Result<Vec<Snapshot>> {
        let mut snapshots: Vec<Snapshot> = read_json_directory(&self.snapshots_dir())?;
        snapshots.retain(|snapshot| snapshot.entry_id == entry_id);
        snapshots.sort_by_key(|snapshot| Reverse(snapshot.created_at_ms));
        Ok(snapshots)
    }

    /// Loads one snapshot manifest.
    ///
    /// # Errors
    ///
    /// Returns an error when the snapshot does not exist or its manifest is invalid.
    pub fn get_snapshot(&self, snapshot_id: &str) -> Result<Snapshot> {
        read_json(&self.snapshot_path(snapshot_id)).map_err(|error| {
            map_not_found(error, || StorageError::SnapshotNotFound(snapshot_id.into()))
        })
    }

    /// Archives an entry's current contents and writes an immutable snapshot manifest.
    ///
    /// # Errors
    ///
    /// Returns an error when the source cannot be scanned, read, archived, hashed, or committed.
    pub fn create_snapshot(
        &self,
        entry_id: &str,
        title: impl Into<String>,
        device_id: impl Into<String>,
        safety: bool,
    ) -> Result<Snapshot> {
        let entry = self.get_entry(entry_id)?;
        let source = PathBuf::from(&entry.source_path);
        if !source.exists() {
            return Err(StorageError::MissingSource(source));
        }
        let files = build_manifest(&source, entry.kind)?;
        let parent = self.list_snapshots(entry_id)?.into_iter().next();
        let changes = compare_manifests(
            parent.as_ref().map(|snapshot| snapshot.files.as_slice()),
            &files,
        );
        let temporary_object = self.temp_dir().join(format!("{}.zip", Uuid::new_v4()));
        create_archive(&temporary_object, &source, entry.kind)?;
        let object_hash = hash_file(&temporary_object)?;
        let stored_object = self.version_root().join(object_key_from_hash(&object_hash));
        if stored_object.exists() {
            fs::remove_file(&temporary_object)?;
        } else {
            if let Some(parent) = stored_object.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(&temporary_object, &stored_object)?;
        }
        let snapshot = Snapshot {
            id: Uuid::new_v4().to_string(),
            entry_id: entry.id,
            parent_id: parent.map(|snapshot| snapshot.id),
            device_id: device_id.into(),
            title: title.into(),
            created_at_ms: unix_millis(SystemTime::now()),
            object_hash,
            size_bytes: stored_object.metadata()?.len(),
            files,
            changes,
            safety,
        };
        write_json_atomic(
            &self.snapshot_path(&snapshot.id),
            &snapshot,
            &self.temp_dir(),
        )?;
        Ok(snapshot)
    }

    /// Verifies that a snapshot object is present and matches its recorded SHA-256 hash.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest or stored object cannot be read.
    pub fn verify_snapshot(&self, snapshot_id: &str) -> Result<bool> {
        let snapshot = self.get_snapshot(snapshot_id)?;
        let object = self.version_root().join(object_key(&snapshot));
        Ok(object.exists() && hash_file(&object)? == snapshot.object_hash)
    }

    /// Restores a snapshot after creating a safety snapshot of the current source.
    ///
    /// # Errors
    ///
    /// Returns an error when verification, safety capture, extraction, or atomic replacement fails.
    pub fn restore_snapshot(&self, entry_id: &str, snapshot_id: &str) -> Result<()> {
        let entry = self.get_entry(entry_id)?;
        let snapshot = self.get_snapshot(snapshot_id)?;
        if snapshot.entry_id != entry.id {
            return Err(StorageError::SnapshotNotFound(snapshot_id.into()));
        }
        let source = PathBuf::from(&entry.source_path);
        if source.exists() {
            self.create_snapshot(entry_id, "恢复前安全快照", "local", true)?;
        }
        if !self.verify_snapshot(snapshot_id)? {
            return Err(StorageError::IntegrityMismatch);
        }
        let staging = source.with_file_name(format!(".chronicle-restore-{}", Uuid::new_v4()));
        extract_archive(&self.version_root().join(object_key(&snapshot)), &staging)?;
        match entry.kind {
            EntryKind::Directory => replace_path(&source, &staging, true),
            EntryKind::File => {
                let archived = staging.join(
                    source
                        .file_name()
                        .ok_or_else(|| StorageError::MissingName(source.clone()))?,
                );
                let result = replace_path(&source, &archived, false);
                if staging.exists() {
                    fs::remove_dir_all(staging)?;
                }
                result
            }
        }
    }

    fn initialize(&self) -> Result<()> {
        for directory in [
            self.entries_dir(),
            self.snapshots_dir(),
            self.objects_dir(),
            self.temp_dir(),
        ] {
            fs::create_dir_all(directory)?;
        }
        let repository_manifest = self.version_root().join("repository.json");
        if !repository_manifest.exists() {
            write_json_atomic(
                &repository_manifest,
                &serde_json::json!({ "format_version": 1 }),
                &self.temp_dir(),
            )?;
        }
        Ok(())
    }

    fn version_root(&self) -> PathBuf {
        self.root.join(FORMAT_VERSION)
    }
    fn entries_dir(&self) -> PathBuf {
        self.version_root().join("entries")
    }
    fn snapshots_dir(&self) -> PathBuf {
        self.version_root().join("snapshots")
    }
    fn objects_dir(&self) -> PathBuf {
        self.version_root().join("objects")
    }
    fn temp_dir(&self) -> PathBuf {
        self.version_root().join("tmp")
    }
    fn entry_path(&self, id: &str) -> PathBuf {
        self.entries_dir().join(format!("{id}.json"))
    }
    fn snapshot_path(&self, id: &str) -> PathBuf {
        self.snapshots_dir().join(format!("{id}.json"))
    }
}

fn object_key_from_hash(hash: &str) -> PathBuf {
    PathBuf::from("objects")
        .join(hash.get(..2).unwrap_or("00"))
        .join(hash)
}

fn build_manifest(source: &Path, kind: EntryKind) -> Result<Vec<SnapshotFile>> {
    let paths = match kind {
        EntryKind::File => vec![source.to_path_buf()],
        EntryKind::Directory => WalkDir::new(source)
            .min_depth(1)
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(entry) if entry.file_type().is_file() => Some(Ok(entry.into_path())),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    let mut files = paths
        .into_iter()
        .map(|path| {
            let relative = if kind == EntryKind::File {
                path.file_name()
                    .map(PathBuf::from)
                    .ok_or_else(|| StorageError::MissingName(path.clone()))?
            } else {
                path.strip_prefix(source)
                    .expect("walked path must be below source")
                    .to_path_buf()
            };
            let metadata = path.metadata()?;
            Ok(SnapshotFile {
                relative_path: archive_path(&relative)?,
                content_hash: hash_file(&path)?,
                size_bytes: metadata.len(),
                modified_at_ms: metadata.modified().map(unix_millis).unwrap_or_default(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(files)
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
    let added = new.keys().filter(|path| !old.contains_key(**path)).count() as u64;
    let modified = new
        .iter()
        .filter(|(path, hash)| old.get(**path).is_some_and(|old_hash| old_hash != *hash))
        .count() as u64;
    let deleted = old.keys().filter(|path| !new.contains_key(**path)).count() as u64;
    ChangeSummary {
        added,
        modified,
        deleted,
    }
}

fn create_archive(destination: &Path, source: &Path, kind: EntryKind) -> Result<()> {
    let mut archive = ZipWriter::new(File::create(destination)?);
    let file_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let directory_options = SimpleFileOptions::default().unix_permissions(0o755);
    if kind == EntryKind::File {
        let name = source
            .file_name()
            .ok_or_else(|| StorageError::MissingName(source.into()))?;
        archive.start_file(archive_path(Path::new(name))?, file_options)?;
        io::copy(&mut BufReader::new(File::open(source)?), &mut archive)?;
    } else {
        let mut entries = WalkDir::new(source)
            .min_depth(1)
            .into_iter()
            .collect::<std::result::Result<Vec<_>, _>>()?;
        entries.sort_by_key(walkdir::DirEntry::depth);
        for entry in entries {
            let relative = entry
                .path()
                .strip_prefix(source)
                .expect("walked path must be below source");
            let name = archive_path(relative)?;
            if entry.file_type().is_dir() {
                archive.add_directory(format!("{name}/"), directory_options)?;
            } else if entry.file_type().is_file() {
                archive.start_file(name, file_options)?;
                io::copy(&mut BufReader::new(File::open(entry.path())?), &mut archive)?;
            }
        }
    }
    archive.finish()?.sync_all()?;
    Ok(())
}

fn extract_archive(object: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let mut archive = ZipArchive::new(BufReader::new(File::open(object)?))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .ok_or(StorageError::UnsafeArchivePath)?;
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(output)?;
        } else {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            io::copy(&mut entry, &mut File::create(output)?)?;
        }
    }
    Ok(())
}

fn replace_path(target: &Path, replacement: &Path, directory: bool) -> Result<()> {
    let old = target.with_file_name(format!(".chronicle-old-{}", Uuid::new_v4()));
    if target.exists() {
        fs::rename(target, &old)?;
    }
    if let Err(error) = fs::rename(replacement, target) {
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

fn read_json_directory<T: DeserializeOwned>(directory: &Path) -> Result<Vec<T>> {
    let mut paths = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "json")
    });
    paths.sort();
    paths.into_iter().map(|path| read_json(&path)).collect()
}

fn write_json_atomic<T: Serialize>(
    path: &Path,
    value: &T,
    temporary_directory: &Path,
) -> Result<()> {
    fs::create_dir_all(temporary_directory)?;
    let temporary = temporary_directory.join(format!("{}.json", Uuid::new_v4()));
    let file = File::create(&temporary)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| StorageError::NonUtf8Path(path.into()))
}

fn archive_path(path: &Path) -> Result<String> {
    let parts = path
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .ok_or_else(|| StorageError::NonUtf8Path(path.into()))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(parts.join("/"))
}

fn unix_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn map_not_found(error: StorageError, not_found: impl FnOnce() -> StorageError) -> StorageError {
    if matches!(&error, StorageError::Io(io_error) if io_error.kind() == io::ErrorKind::NotFound) {
        not_found()
    } else {
        error
    }
}

#[cfg(test)]
mod tests {
    use super::LocalRepository;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn snapshots_detect_changes_and_restore_directory() {
        let workspace = tempdir().unwrap();
        let source = workspace.path().join("source");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("settings.json"), b"version one").unwrap();
        fs::write(source.join("nested/note.txt"), b"keep").unwrap();
        let repository = LocalRepository::open(workspace.path().join("repository")).unwrap();
        let entry = repository
            .add_entry(&source, None, Some("配置".into()))
            .unwrap();
        let first = repository
            .create_snapshot(&entry.id, "初始版本", "test", false)
            .unwrap();
        fs::write(source.join("settings.json"), b"version two").unwrap();
        fs::remove_file(source.join("nested/note.txt")).unwrap();
        fs::write(source.join("added.txt"), b"new").unwrap();
        let second = repository
            .create_snapshot(&entry.id, "修改版本", "test", false)
            .unwrap();
        assert_eq!(second.changes.added, 1);
        assert_eq!(second.changes.modified, 1);
        assert_eq!(second.changes.deleted, 1);
        assert!(repository.verify_snapshot(&first.id).unwrap());
        repository.restore_snapshot(&entry.id, &first.id).unwrap();
        assert_eq!(
            fs::read(source.join("settings.json")).unwrap(),
            b"version one"
        );
        assert_eq!(fs::read(source.join("nested/note.txt")).unwrap(), b"keep");
        assert!(!source.join("added.txt").exists());
        assert_eq!(repository.list_snapshots(&entry.id).unwrap().len(), 3);
    }
}
