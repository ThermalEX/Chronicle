import { invoke, isTauri as detectTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { ArchiveKind, ArchiveRecord, SnapshotProgress, SnapshotRecord } from "../domain";
import { BrowserArchiveRepository } from "./archiveRepository";

export interface ArchiveRepository {
  listArchives(): Promise<ArchiveRecord[]>;
  addArchive(kind: ArchiveKind, category?: string): Promise<ArchiveRecord | undefined>;
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

  async addArchive(kind: ArchiveKind, category = "未分类"): Promise<ArchiveRecord | undefined> {
    const sourcePath = await open({
      directory: kind === "folder",
      multiple: false,
      recursive: kind === "folder",
      title: kind === "folder" ? "选择要管理的文件夹" : "选择要管理的文件",
    });
    if (!sourcePath) return undefined;
    return invoke("add_entry", { sourcePath, categoryId: category });
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
