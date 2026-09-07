import type {
  ArchiveRecord,
  ArchiveSource,
  CategoryRecord,
  CreateArchiveInput,
  SnapshotFile,
  SnapshotProgress,
  SnapshotRecord,
  RepositoryInfo,
  RecycleItem,
  SourceKind,
} from "../domain";

const DATABASE_NAME = "chronicle-local";
const DATABASE_VERSION = 3;
const ARCHIVES = "archives";
const SNAPSHOTS = "snapshots";
const CATEGORIES = "categories";
const TRASH = "trash";

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
    if (!database.objectStoreNames.contains(CATEGORIES)) {
      database.createObjectStore(CATEGORIES, { keyPath: "id" });
    }
    if (!database.objectStoreNames.contains(TRASH)) {
      database.createObjectStore(TRASH, { keyPath: "id" });
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

async function readSources(archive: ArchiveRecord): Promise<Array<{ path: string; file: File }>> {
  const files: Array<{ path: string; file: File }> = [];
  for (const source of archive.sources) {
    if (!source.handle) throw new Error(`“${source.name}”缺少本地文件句柄`);
    await ensurePermission(source.handle, "read");
    if (source.kind === "file") {
      const file = await (source.handle as FileSystemFileHandle).getFile();
      files.push({ path: `${source.id}/${file.name}`, file });
    } else {
      const children = await collectDirectory(source.handle as FileSystemDirectoryHandle);
      files.push(...children.map((item) => ({ ...item, path: `${source.id}/${item.path}` })));
    }
  }
  return files.sort((left, right) => left.path.localeCompare(right.path));
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
  async getRepositoryInfo(): Promise<RepositoryInfo> {
    const database = await openDatabase();
    const transaction = database.transaction(SNAPSHOTS, "readonly");
    const snapshots = await requestResult(transaction.objectStore(SNAPSHOTS).getAll() as IDBRequest<SnapshotRecord[]>);
    await transactionDone(transaction);
    database.close();
    return { path: "浏览器本地数据库", totalBytes: snapshots.reduce((total, snapshot) => total + snapshot.totalBytes, 0) };
  }

  async openRepositoryFolder(): Promise<void> {
    throw new Error("浏览器模式没有可打开的资料库文件夹");
  }

  async listRecycleItems(): Promise<RecycleItem[]> {
    const database = await openDatabase();
    const transaction = database.transaction(TRASH, "readonly");
    const records = await requestResult(transaction.objectStore(TRASH).getAll() as IDBRequest<Array<Record<string, unknown>>>);
    await transactionDone(transaction);
    database.close();
    return records.map((record) => {
      const archives = (record.archives as ArchiveRecord[] | undefined) ?? (record.archive ? [record.archive as ArchiveRecord] : []);
      const snapshots = (record.snapshots as SnapshotRecord[] | undefined) ?? [];
      return {
        id: record.id as string,
        kind: record.kind as "archive" | "category",
        displayName: record.kind === "archive" ? archives[0]?.name ?? "存档" : ((record.categories as CategoryRecord[] | undefined)?.[0]?.name ?? "分类"),
        deletedAt: record.deletedAt as number,
        sizeBytes: snapshots.reduce((total, snapshot) => total + snapshot.totalBytes, 0),
        entryCount: archives.length,
      };
    }).sort((left, right) => right.deletedAt - left.deletedAt);
  }

  async restoreRecycleItem(itemId: string): Promise<void> {
    const database = await openDatabase();
    const read = database.transaction(TRASH, "readonly");
    const record = await requestResult(read.objectStore(TRASH).get(itemId));
    await transactionDone(read);
    if (!record) { database.close(); throw new Error("回收站项目不存在"); }
    const archives = (record.archives as ArchiveRecord[] | undefined) ?? (record.archive ? [record.archive as ArchiveRecord] : []);
    const categories = (record.categories as CategoryRecord[] | undefined) ?? [];
    const snapshots = (record.snapshots as SnapshotRecord[] | undefined) ?? [];
    const transaction = database.transaction([ARCHIVES, SNAPSHOTS, CATEGORIES, TRASH], "readwrite");
    for (const category of categories) transaction.objectStore(CATEGORIES).add(category);
    for (const archive of archives) transaction.objectStore(ARCHIVES).add(archive);
    for (const snapshot of snapshots) transaction.objectStore(SNAPSHOTS).add(snapshot);
    transaction.objectStore(TRASH).delete(itemId);
    await transactionDone(transaction);
    database.close();
  }

  async permanentlyDeleteRecycleItem(itemId: string): Promise<void> {
    const database = await openDatabase();
    const transaction = database.transaction(TRASH, "readwrite");
    transaction.objectStore(TRASH).delete(itemId);
    await transactionDone(transaction);
    database.close();
  }

  async emptyRecycleBin(): Promise<void> {
    const database = await openDatabase();
    const transaction = database.transaction(TRASH, "readwrite");
    transaction.objectStore(TRASH).clear();
    await transactionDone(transaction);
    database.close();
  }

  async listArchives(): Promise<ArchiveRecord[]> {
    const database = await openDatabase();
    const transaction = database.transaction(ARCHIVES, "readonly");
    const records = await requestResult(transaction.objectStore(ARCHIVES).getAll() as IDBRequest<ArchiveRecord[]>);
    await transactionDone(transaction);
    database.close();
    return records
      .map((archive) => ({ ...archive, tags: archive.tags ?? [], syncMode: archive.syncMode ?? "manual" }))
      .sort((left, right) => right.updatedAt - left.updatedAt);
  }

  async pickSources(kind: SourceKind): Promise<ArchiveSource[]> {
    try {
      const handles = kind === "file"
        ? await window.showOpenFilePicker({ multiple: true })
        : [await window.showDirectoryPicker({ mode: "read" })];
      return handles.map((handle) => ({
        id: crypto.randomUUID(),
        name: handle.name,
        path: handle.name,
        kind,
        handle,
      }));
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") return [];
      throw error;
    }
  }

  async createArchive(input: CreateArchiveInput): Promise<ArchiveRecord> {
    const now = Date.now();
    const category = input.categoryId ? (await this.listCategories()).find((item) => item.id === input.categoryId) : undefined;
    if (input.categoryId && !category) throw new Error("分类不存在");
    const archive: ArchiveRecord = {
      id: crypto.randomUUID(),
      name: input.name.trim(),
      sourcePath: input.sources.map((source) => source.path).join(" · "),
      sources: input.sources,
      category: category?.name ?? "未分类",
      categoryId: input.categoryId,
      tags: [],
      kind: input.sources.length === 1 ? input.sources[0].kind : "collection",
      storagePolicy: input.storagePolicy,
      syncMode: input.syncMode,
      createdAt: now,
      updatedAt: now,
      totalBytes: 0,
    };
    await this.putArchive(archive);
    return archive;
  }

  async updateArchive(archiveId: string, input: CreateArchiveInput): Promise<ArchiveRecord> {
    const archive = (await this.listArchives()).find((item) => item.id === archiveId);
    if (!archive) throw new Error("存档不存在");
    const updated: ArchiveRecord = {
      ...archive,
      name: input.name.trim(),
      sourcePath: input.sources.map((source) => source.path).join(" · "),
      sources: input.sources,
      kind: input.sources.length === 1 ? input.sources[0].kind : "collection",
      storagePolicy: input.storagePolicy,
      syncMode: input.syncMode,
      updatedAt: Date.now(),
    };
    await this.putArchive(updated);
    return updated;
  }

  async deleteArchive(archiveId: string, recycleBinEnabled: boolean): Promise<void> {
    const archive = (await this.listArchives()).find((item) => item.id === archiveId);
    if (!archive) throw new Error("存档不存在");
    const snapshots = await this.listSnapshots(archiveId);
    const database = await openDatabase();
    const transaction = database.transaction([ARCHIVES, SNAPSHOTS, TRASH], "readwrite");
    if (recycleBinEnabled) transaction.objectStore(TRASH).put({ id: crypto.randomUUID(), kind: "archive", deletedAt: Date.now(), archive, snapshots });
    transaction.objectStore(ARCHIVES).delete(archiveId);
    for (const snapshot of snapshots) transaction.objectStore(SNAPSHOTS).delete(snapshot.id);
    await transactionDone(transaction);
    database.close();
  }

  async deleteCategory(categoryId: string, recycleBinEnabled: boolean): Promise<void> {
    const categories = await this.listCategories();
    if (!categories.some((item) => item.id === categoryId)) throw new Error("分类不存在");
    const ids = new Set([categoryId]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const category of categories) {
        if (category.parentId && ids.has(category.parentId) && !ids.has(category.id)) {
          ids.add(category.id);
          changed = true;
        }
      }
    }
    const archives = (await this.listArchives()).filter((archive) => archive.categoryId && ids.has(archive.categoryId));
    const snapshots = (await Promise.all(archives.map((archive) => this.listSnapshots(archive.id)))).flat();
    const database = await openDatabase();
    const transaction = database.transaction([ARCHIVES, SNAPSHOTS, CATEGORIES, TRASH], "readwrite");
    if (recycleBinEnabled) transaction.objectStore(TRASH).put({ id: crypto.randomUUID(), kind: "category", deletedAt: Date.now(), categories: categories.filter((item) => ids.has(item.id)), archives, snapshots });
    for (const category of categories) if (ids.has(category.id)) transaction.objectStore(CATEGORIES).delete(category.id);
    for (const archive of archives) transaction.objectStore(ARCHIVES).delete(archive.id);
    for (const snapshot of snapshots) transaction.objectStore(SNAPSHOTS).delete(snapshot.id);
    await transactionDone(transaction);
    database.close();
  }

  async listCategories(): Promise<CategoryRecord[]> {
    const database = await openDatabase();
    const transaction = database.transaction(CATEGORIES, "readonly");
    const records = await requestResult(transaction.objectStore(CATEGORIES).getAll() as IDBRequest<CategoryRecord[]>);
    await transactionDone(transaction);
    database.close();
    return records;
  }

  async createCategory(name: string, parentId?: string): Promise<CategoryRecord> {
    const category = { id: crypto.randomUUID(), name: name.trim(), parentId };
    const database = await openDatabase();
    const transaction = database.transaction(CATEGORIES, "readwrite");
    transaction.objectStore(CATEGORIES).put(category);
    await transactionDone(transaction);
    database.close();
    return category;
  }

  async moveCategory(categoryId: string, parentId?: string): Promise<void> {
    const categories = await this.listCategories();
    const category = categories.find((item) => item.id === categoryId);
    if (!category || (parentId && !categories.some((item) => item.id === parentId))) throw new Error("分类不存在");
    let ancestor: string | null | undefined = parentId;
    while (ancestor) {
      if (ancestor === categoryId) throw new Error("不能把分类移入自身或自己的子分类");
      ancestor = categories.find((item) => item.id === ancestor)?.parentId;
    }
    const database = await openDatabase();
    const transaction = database.transaction(CATEGORIES, "readwrite");
    transaction.objectStore(CATEGORIES).put({ ...category, parentId });
    await transactionDone(transaction);
    database.close();
  }

  async setArchiveCategory(archiveId: string, categoryId?: string): Promise<void> {
    const archives = await this.listArchives();
    const archive = archives.find((item) => item.id === archiveId);
    if (!archive) throw new Error("存档不存在");
    const category = categoryId ? (await this.listCategories()).find((item) => item.id === categoryId) : undefined;
    if (categoryId && !category) throw new Error("分类不存在");
    await this.putArchive({ ...archive, category: category?.name ?? "未分类", categoryId, updatedAt: Date.now() });
  }

  async setArchiveTags(archiveId: string, tags: string[]): Promise<void> {
    const archives = await this.listArchives();
    const archive = archives.find((item) => item.id === archiveId);
    if (!archive) throw new Error("存档不存在");
    const normalized = tags
      .map((tag) => tag.trim())
      .filter((tag, index, items) => tag && items.findIndex((item) => item.toLocaleLowerCase() === tag.toLocaleLowerCase()) === index);
    await this.putArchive({ ...archive, tags: normalized, updatedAt: Date.now() });
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
    const sourceFiles = await readSources(archive);
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
      deviceId: "browser",
      deviceName: "浏览器设备",
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
    await this.createSnapshot(archive, "恢复前安全快照", true);
    for (const source of archive.sources) {
      if (!source.handle) throw new Error(`“${source.name}”缺少本地文件句柄`);
      await ensurePermission(source.handle, "readwrite");
      const prefix = `${source.id}/`;
      const files = snapshot.files.filter((file) => file.path.startsWith(prefix));
      if (source.kind === "file") {
        const file = files.find((item) => item.path === `${prefix}${source.name}`);
        if (!file?.blob) throw new Error(`快照中缺少“${source.name}”`);
        await writeFile(source.handle as FileSystemFileHandle, file.blob);
        continue;
      }
      const root = source.handle as FileSystemDirectoryHandle;
      await removeContents(root);
      for (const file of files) {
        if (!file.blob) throw new Error(`快照文件缺少内容：${file.path}`);
        await writeFile(await ensureFile(root, file.path.slice(prefix.length)), file.blob);
      }
    }
  }
}
