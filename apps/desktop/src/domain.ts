export type SourceKind = "file" | "folder";
export type ArchiveKind = SourceKind | "collection";
export type StoragePolicy = "local" | "local_and_remote";

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
  storagePolicy: StoragePolicy;
  createInitialSnapshot: boolean;
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
  createdAt: number;
  totalBytes: number;
  contentHash: string;
  files: SnapshotFile[];
  changes: { added: number; modified: number; deleted: number };
  safety: boolean;
};

export type SnapshotProgress = {
  current: number;
  total: number;
  currentPath: string;
};
