export type SourceKind = "file" | "folder";
export type ArchiveKind = SourceKind | "collection";
export type StoragePolicy = "local" | "local_and_remote";
export type ArchiveSyncMode = "manual" | "automatic";

export type CategoryRecord = {
  id: string;
  name: string;
  parentId?: string | null;
};

export type ArchiveSource = {
  id: string;
  name: string;
  path: string;
  kind: SourceKind;
  handle?: FileSystemFileHandle | FileSystemDirectoryHandle;
};

export type CreateArchiveInput = {
  name: string;
  sources: ArchiveSource[];
  categoryId?: string;
  storagePolicy: StoragePolicy;
  createInitialSnapshot: boolean;
  syncMode: ArchiveSyncMode;
  autoBackupEnabled: boolean;
  automaticUploadEnabled: boolean;
};

export type ArchiveRecord = {
  id: string;
  name: string;
  sourcePath: string;
  sources: ArchiveSource[];
  category: string;
  categoryId?: string;
  tags: string[];
  kind: ArchiveKind;
  storagePolicy: StoragePolicy;
  syncMode: ArchiveSyncMode;
  autoBackupEnabled: boolean;
  automaticUploadEnabled: boolean;
  createdAt: number;
  updatedAt: number;
  totalBytes: number;
  lastSnapshotAt?: number;
};

export type SnapshotFile = {
  path: string;
  blob?: Blob;
  size: number;
  lastModified: number;
  hash: string;
};

export type SnapshotRecord = {
  id: string;
  archiveId: string;
  title: string;
  note?: string;
  createdAt: number;
  totalBytes: number;
  contentHash: string;
  files: SnapshotFile[];
  changes: { added: number; modified: number; deleted: number };
  safety: boolean;
  deviceId?: string;
  deviceName?: string;
};

export type SnapshotProgress = {
  current: number;
  total: number;
  currentPath: string;
};

export type RepositoryInfo = {
  path: string;
  totalBytes: number;
};

export type RecycleItem = {
  id: string;
  kind: "archive" | "category";
  displayName: string;
  deletedAt: number;
  sizeBytes: number;
  entryCount: number;
};
