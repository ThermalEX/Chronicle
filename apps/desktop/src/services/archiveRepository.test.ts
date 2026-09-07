import "fake-indexeddb/auto";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ArchiveRecord } from "../domain";
import { BrowserArchiveRepository } from "./archiveRepository";

class MemoryFileHandle {
  readonly kind = "file" as const;

  constructor(readonly name: string, private content: string) {}

  async queryPermission() { return "granted" as const; }
  async requestPermission() { return "granted" as const; }

  async getFile() {
    return new File([this.content], this.name, { lastModified: 1 });
  }

  async createWritable() {
    return {
      write: async (blob: Blob) => { this.content = await blob.text(); },
      close: async () => undefined,
    };
  }

  setContent(content: string) { this.content = content; }
  readContent() { return this.content; }
}

function archiveFor(handle: MemoryFileHandle): ArchiveRecord {
  const sourceId = crypto.randomUUID();
  return {
    id: crypto.randomUUID(),
    name: handle.name,
    sourcePath: handle.name,
    sources: [{
      id: sourceId,
      name: handle.name,
      path: handle.name,
      kind: "file",
      handle: handle as unknown as FileSystemFileHandle,
    }],
    category: "未分类",
    tags: [],
    kind: "file",
    storagePolicy: "local",
    syncMode: "manual",
    createdAt: 1,
    updatedAt: 1,
    totalBytes: 0,
  };
}

afterEach(async () => {
  vi.unstubAllGlobals();
  await new Promise<void>((resolve, reject) => {
    const request = indexedDB.deleteDatabase("chronicle-local");
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
});

describe("ArchiveRepository", () => {
  it("stores a handle returned by the picker without requesting permission again", async () => {
    const repository = new BrowserArchiveRepository();
    const handle = new MemoryFileHandle("settings.json", "version one");
    const permissionQuery = vi.spyOn(handle, "queryPermission");
    vi.stubGlobal("window", { showOpenFilePicker: vi.fn().mockResolvedValue([handle]) });

    const sources = await repository.pickSources("file");
    const archive = await repository.createArchive({
      name: "设置",
      sources,
      storagePolicy: "local",
      syncMode: "manual",
      createInitialSnapshot: false,
    });

    expect(archive.name).toBe("设置");
    expect(archive.category).toBe("未分类");
    expect(permissionQuery).not.toHaveBeenCalled();

    await repository.setArchiveTags(archive.id, ["重要", "配置", "重要"]);
    const [updated] = await repository.listArchives();
    expect(updated.tags).toEqual(["重要", "配置"]);
  });

  it("creates versioned snapshots and restores an earlier file", async () => {
    const repository = new BrowserArchiveRepository();
    const handle = new MemoryFileHandle("settings.json", "version one");
    const archive = archiveFor(handle);
    await repository.putArchive(archive);

    const first = await repository.createSnapshot(archive, "初始版本");
    handle.setContent("version two");
    const second = await repository.createSnapshot(archive, "修改版本");

    expect(first.files).toHaveLength(1);
    expect(second.changes).toEqual({ added: 0, modified: 1, deleted: 0 });
    expect(second.contentHash).not.toBe(first.contentHash);

    await repository.restoreSnapshot(archive, first);

    expect(handle.readContent()).toBe("version one");
    const history = await repository.listSnapshots(archive.id);
    expect(history).toHaveLength(3);
    expect(history[0].title).toBe("恢复前安全快照");
    expect(history[0].safety).toBe(true);
  });
});
