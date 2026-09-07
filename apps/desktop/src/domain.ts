export type ArchiveKind = "file" | "folder";

export type ArchiveRecord = {
  id: string;
  name: string;
  sourcePath: string;
  category: string;
  kind: ArchiveKind;
  handle?: FileSystemFileHandle | FileSystemDirectoryHandle;
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
