import { invoke, isTauri as detectTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  ArchiveRecord,
  ArchiveSource,
  CategoryRecord,
  CreateArchiveInput,
  SnapshotProgress,
  SnapshotRecord,
  RepositoryInfo,
  RecycleItem,
  SourceKind,
} from "../domain";
import { BrowserArchiveRepository } from "./archiveRepository";

export interface ArchiveRepository {
  listArchives(): Promise<ArchiveRecord[]>;
  pickSources(kind: SourceKind): Promise<ArchiveSource[]>;
  createArchive(input: CreateArchiveInput): Promise<ArchiveRecord>;
  updateArchive(archiveId: string, input: CreateArchiveInput): Promise<ArchiveRecord>;
  deleteArchive(archiveId: string, recycleBinEnabled: boolean, recycleBinPath?: string): Promise<void>;
  deleteCategory(categoryId: string, recycleBinEnabled: boolean, recycleBinPath?: string): Promise<void>;
  listCategories(): Promise<CategoryRecord[]>;
  createCategory(name: string, parentId?: string): Promise<CategoryRecord>;
  moveCategory(categoryId: string, parentId?: string): Promise<void>;
  setArchiveCategory(archiveId: string, categoryId?: string): Promise<void>;
  setArchiveTags(archiveId: string, tags: string[]): Promise<void>;
  getRepositoryInfo(): Promise<RepositoryInfo>;
  openRepositoryFolder(): Promise<void>;
  openRecycleBin(recycleBinPath?: string): Promise<void>;
  listRecycleItems(): Promise<RecycleItem[]>;
  restoreRecycleItem(itemId: string): Promise<void>;
  permanentlyDeleteRecycleItem(itemId: string): Promise<void>;
  emptyRecycleBin(): Promise<void>;
  listSnapshots(archiveId: string): Promise<SnapshotRecord[]>;
  createSnapshot(
    archive: ArchiveRecord,
    title?: string,
    safety?: boolean,
    onProgress?: (progress: SnapshotProgress) => void,
  ): Promise<SnapshotRecord>;
  restoreSnapshot(archive: ArchiveRecord, snapshot: SnapshotRecord): Promise<void>;
}

class TauriArchiveRepository implements ArchiveRepository {
  listArchives(): Promise<ArchiveRecord[]> {
    return invoke("list_entries");
  }

  async pickSources(kind: SourceKind): Promise<ArchiveSource[]> {
    const selected = await open({
      directory: kind === "folder",
      multiple: true,
      recursive: kind === "folder",
      title: kind === "folder" ? "选择存档文件夹" : "选择存档文件",
    });
    const paths = typeof selected === "string" ? [selected] : selected ?? [];
    return paths.map((path) => ({
      id: crypto.randomUUID(),
      name: path.split(/[\\/]/).at(-1) || path,
      path,
      kind,
    }));
  }

  createArchive(input: CreateArchiveInput): Promise<ArchiveRecord> {
    return invoke("add_entry", {
      name: input.name,
      sourcePaths: input.sources.map((source) => source.path),
      categoryId: input.categoryId ?? null,
      storagePolicy: input.storagePolicy,
      syncMode: input.syncMode,
    });
  }

  updateArchive(archiveId: string, input: CreateArchiveInput): Promise<ArchiveRecord> {
    return invoke("update_entry", {
      entryId: archiveId,
      name: input.name,
      sourcePaths: input.sources.map((source) => source.path),
      storagePolicy: input.storagePolicy,
      syncMode: input.syncMode,
    });
  }

  deleteArchive(archiveId: string, recycleBinEnabled: boolean, recycleBinPath?: string): Promise<void> {
    return invoke("delete_entry", { entryId: archiveId, recycleBinEnabled, recycleBinPath: recycleBinPath || null });
  }

  deleteCategory(categoryId: string, recycleBinEnabled: boolean, recycleBinPath?: string): Promise<void> {
    return invoke("delete_category", { categoryId, recycleBinEnabled, recycleBinPath: recycleBinPath || null });
  }

  listCategories(): Promise<CategoryRecord[]> {
    return invoke("list_categories");
  }

  createCategory(name: string, parentId?: string): Promise<CategoryRecord> {
    return invoke("create_category", { name, parentId: parentId ?? null });
  }

  moveCategory(categoryId: string, parentId?: string): Promise<void> {
    return invoke("move_category", { categoryId, parentId: parentId ?? null });
  }

  setArchiveCategory(archiveId: string, categoryId?: string): Promise<void> {
    return invoke("set_entry_category", { entryId: archiveId, categoryId: categoryId ?? null });
  }

  setArchiveTags(archiveId: string, tags: string[]): Promise<void> {
    return invoke("set_entry_tags", { entryId: archiveId, tags });
  }

  getRepositoryInfo(): Promise<RepositoryInfo> {
    return invoke("repository_info");
  }

  openRepositoryFolder(): Promise<void> {
    return invoke("open_repository_folder");
  }

  openRecycleBin(recycleBinPath?: string): Promise<void> {
    return invoke("open_recycle_bin", { recycleBinPath: recycleBinPath || null });
  }

  listRecycleItems(): Promise<RecycleItem[]> {
    return invoke("list_recycle_items");
  }

  restoreRecycleItem(itemId: string): Promise<void> {
    return invoke("restore_recycle_item", { itemId });
  }

  permanentlyDeleteRecycleItem(itemId: string): Promise<void> {
    return invoke("permanently_delete_recycle_item", { itemId });
  }

  emptyRecycleBin(): Promise<void> {
    return invoke("empty_recycle_bin");
  }

  listSnapshots(archiveId: string): Promise<SnapshotRecord[]> {
    return invoke("list_snapshots", { entryId: archiveId });
  }

  async createSnapshot(
    archive: ArchiveRecord,
    title = "手动备份",
    safety = false,
    onProgress?: (progress: SnapshotProgress) => void,
  ): Promise<SnapshotRecord> {
    onProgress?.({ current: 0, total: 1, currentPath: "正在由本地仓库创建归档" });
    const snapshot = await invoke<SnapshotRecord>("create_snapshot", {
      entryId: archive.id,
      title,
      safety,
    });
    onProgress?.({ current: 1, total: 1, currentPath: "" });
    return snapshot;
  }

  restoreSnapshot(archive: ArchiveRecord, snapshot: SnapshotRecord): Promise<void> {
    return invoke("restore_snapshot", { entryId: archive.id, snapshotId: snapshot.id });
  }
}

export const isTauriRuntime = detectTauri();
export const archiveRepository: ArchiveRepository = isTauriRuntime
  ? new TauriArchiveRepository()
  : new BrowserArchiveRepository();
