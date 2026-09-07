import type { ArchiveKind, ArchiveRecord, SnapshotFile, SnapshotProgress, SnapshotRecord } from "../domain";

const DATABASE_NAME = "chronicle-local";
const DATABASE_VERSION = 1;
const ARCHIVES = "archives";
const SNAPSHOTS = "snapshots";

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("本地数据库操作失败"));
  });
}

function transactionDone(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error ?? new Error("本地数据库事务失败"));
    transaction.onabort = () => reject(transaction.error ?? new Error("本地数据库事务已中止"));
  });
}

async function openDatabase(): Promise<IDBDatabase> {
  const request = indexedDB.open(DATABASE_NAME, DATABASE_VERSION);
  request.onupgradeneeded = () => {
    const database = request.result;
    if (!database.objectStoreNames.contains(ARCHIVES)) {
      database.createObjectStore(ARCHIVES, { keyPath: "id" });
    }
    if (!database.objectStoreNames.contains(SNAPSHOTS)) {
      const store = database.createObjectStore(SNAPSHOTS, { keyPath: "id" });
      store.createIndex("archiveId", "archiveId");
    }
  };
  return requestResult(request);
}

async function digest(blob: Blob): Promise<string> {
  const bytes = await blob.arrayBuffer();
  const hash = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(hash), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function collectDirectory(
  directory: FileSystemDirectoryHandle,
  prefix = "",
): Promise<Array<{ path: string; file: File }>> {
  const files: Array<{ path: string; file: File }> = [];
  for await (const [name, handle] of directory.entries()) {
    const path = prefix ? `${prefix}/${name}` : name;
    if (handle.kind === "file") {
      files.push({ path, file: await (handle as FileSystemFileHandle).getFile() });
    } else {
      files.push(...await collectDirectory(handle as FileSystemDirectoryHandle, path));
    }
  }
  return files.sort((left, right) => left.path.localeCompare(right.path));
}

async function readSource(archive: ArchiveRecord): Promise<Array<{ path: string; file: File }>> {
  if (!archive.handle) throw new Error("存档缺少本地文件句柄");
  if (archive.kind === "file") {
    const file = await (archive.handle as FileSystemFileHandle).getFile();
    return [{ path: file.name, file }];
  }
  return collectDirectory(archive.handle as FileSystemDirectoryHandle);
}

async function ensurePermission(handle: FileSystemHandle, mode: "read" | "readwrite"): Promise<void> {
  if (await handle.queryPermission({ mode }) === "granted") return;
  if (await handle.requestPermission({ mode }) !== "granted") {
    throw new Error(mode === "read" ? "未获得读取权限" : "未获得写入权限");
  }
}

async function writeFile(handle: FileSystemFileHandle, blob: Blob): Promise<void> {
  const writable = await handle.createWritable();
  await writable.write(blob);
  await writable.close();
}

async function ensureFile(
  root: FileSystemDirectoryHandle,
  path: string,
): Promise<FileSystemFileHandle> {
  const parts = path.split("/");
  const filename = parts.pop();
  if (!filename) throw new Error("快照包含无效路径");
  let directory = root;
  for (const part of parts) directory = await directory.getDirectoryHandle(part, { create: true });
  return directory.getFileHandle(filename, { create: true });
}

async function removeContents(directory: FileSystemDirectoryHandle): Promise<void> {
  for await (const [name] of directory.entries()) await directory.removeEntry(name, { recursive: true });
}

export class BrowserArchiveRepository {
  async listArchives(): Promise<ArchiveRecord[]> {
    const database = await openDatabase();
    const transaction = database.transaction(ARCHIVES, "readonly");
    const records = await requestResult(transaction.objectStore(ARCHIVES).getAll() as IDBRequest<ArchiveRecord[]>);
    await transactionDone(transaction);
    database.close();
    return records.sort((left, right) => right.updatedAt - left.updatedAt);
  }

  async addArchive(kind: ArchiveKind, category = "未分类"): Promise<ArchiveRecord | undefined> {
    let handle: FileSystemFileHandle | FileSystemDirectoryHandle;
    try {
      handle = kind === "file"
        ? (await window.showOpenFilePicker({ multiple: false }))[0]
        : await window.showDirectoryPicker({ mode: "readwrite" });
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") return undefined;
      throw error;
    }
    if (!handle) return undefined;
    await ensurePermission(handle, "read");
    const now = Date.now();
    const archive: ArchiveRecord = {
      id: crypto.randomUUID(),
      name: handle.name,
      sourcePath: handle.name,
      category,
      kind,
      handle,
      createdAt: now,
      updatedAt: now,
      totalBytes: 0,
    };
    await this.putArchive(archive);
    return archive;
  }

  async putArchive(archive: ArchiveRecord): Promise<void> {
    const database = await openDatabase();
    const transaction = database.transaction(ARCHIVES, "readwrite");
    transaction.objectStore(ARCHIVES).put(archive);
    await transactionDone(transaction);
    database.close();
  }

  async listSnapshots(archiveId: string): Promise<SnapshotRecord[]> {
    const database = await openDatabase();
    const transaction = database.transaction(SNAPSHOTS, "readonly");
    const index = transaction.objectStore(SNAPSHOTS).index("archiveId");
    const records = await requestResult(index.getAll(archiveId) as IDBRequest<SnapshotRecord[]>);
    await transactionDone(transaction);
    database.close();
    return records.sort((left, right) => right.createdAt - left.createdAt);
  }

  async createSnapshot(
    archive: ArchiveRecord,
    title = "手动备份",
    safety = false,
    onProgress?: (progress: SnapshotProgress) => void,
  ): Promise<SnapshotRecord> {
    if (!archive.handle) throw new Error("存档缺少本地文件句柄");
    await ensurePermission(archive.handle, "read");
    const sourceFiles = await readSource(archive);
    const files: SnapshotFile[] = [];
    for (const [index, item] of sourceFiles.entries()) {
      onProgress?.({ current: index, total: sourceFiles.length, currentPath: item.path });
      files.push({
        path: item.path,
        blob: item.file,
        size: item.file.size,
        lastModified: item.file.lastModified,
        hash: await digest(item.file),
      });
    }
    onProgress?.({ current: sourceFiles.length, total: sourceFiles.length, currentPath: "" });
    const previous = (await this.listSnapshots(archive.id))[0];
    const previousFiles = new Map(previous?.files.map((file) => [file.path, file]));
    const currentFiles = new Map(files.map((file) => [file.path, file]));
    const changes = { added: 0, modified: 0, deleted: 0 };
    for (const file of files) {
      const oldFile = previousFiles.get(file.path);
      if (!oldFile) changes.added += 1;
      else if (oldFile.hash !== file.hash) changes.modified += 1;
    }
    for (const path of previousFiles.keys()) if (!currentFiles.has(path)) changes.deleted += 1;
    const manifest = files.map((file) => `${file.path}:${file.hash}`).join("\n");
    const snapshot: SnapshotRecord = {
      id: crypto.randomUUID(),
      archiveId: archive.id,
      title,
      createdAt: Date.now(),
      totalBytes: files.reduce((total, file) => total + file.size, 0),
      contentHash: await digest(new Blob([manifest])),
      files,
      changes,
      safety,
    };
    const database = await openDatabase();
    const transaction = database.transaction(SNAPSHOTS, "readwrite");
    transaction.objectStore(SNAPSHOTS).put(snapshot);
    await transactionDone(transaction);
    database.close();
    await this.putArchive({
      ...archive,
      updatedAt: snapshot.createdAt,
      lastSnapshotAt: snapshot.createdAt,
      totalBytes: snapshot.totalBytes,
    });
    return snapshot;
  }

  async restoreSnapshot(archive: ArchiveRecord, snapshot: SnapshotRecord): Promise<void> {
    if (!archive.handle) throw new Error("存档缺少本地文件句柄");
    await ensurePermission(archive.handle, "readwrite");
    await this.createSnapshot(archive, "恢复前安全快照", true);
    if (archive.kind === "file") {
      const file = snapshot.files[0];
      if (!file?.blob) throw new Error("快照中没有可恢复的文件");
      await writeFile(archive.handle as FileSystemFileHandle, file.blob);
      return;
    }
    const root = archive.handle as FileSystemDirectoryHandle;
    await removeContents(root);
    for (const file of snapshot.files) {
      if (!file.blob) throw new Error(`快照文件缺少内容：${file.path}`);
      await writeFile(await ensureFile(root, file.path), file.blob);
    }
  }
}
