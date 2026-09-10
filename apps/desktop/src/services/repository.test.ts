import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const open = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => true }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));

describe("desktop repository adapter", () => {
  beforeEach(() => {
    invoke.mockReset();
    open.mockReset();
  });

  it("uses the native dialog and Rust command in a Tauri runtime", async () => {
    open.mockResolvedValue(["C:\\Users\\ThermalEX\\Documents\\Example", "C:\\Users\\ThermalEX\\settings.json"]);
    invoke.mockResolvedValue({ id: "entry-1", name: "Example" });
    const { archiveRepository, isTauriRuntime } = await import("./repository");

    const sources = await archiveRepository.pickSources("folder");
    const entry = await archiveRepository.createArchive({
      name: "Example",
      sources,
      storagePolicy: "local_and_remote",
      syncMode: "manual",
      autoBackupEnabled: false,
      automaticUploadEnabled: false,
      createInitialSnapshot: true,
    });

    expect(isTauriRuntime).toBe(true);
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ directory: true, multiple: true }));
    expect(invoke).toHaveBeenCalledWith("add_entry", {
      name: "Example",
      sourcePaths: ["C:\\Users\\ThermalEX\\Documents\\Example", "C:\\Users\\ThermalEX\\settings.json"],
      categoryId: null,
      storagePolicy: "local_and_remote",
      syncMode: "manual",
    });
    expect(entry).toEqual({ id: "entry-1", name: "Example" });

    await archiveRepository.setArchiveTags("entry-1", ["工作", "重要"]);
    expect(invoke).toHaveBeenLastCalledWith("set_entry_tags", {
      entryId: "entry-1",
      tags: ["工作", "重要"],
    });

    invoke.mockResolvedValue([{ id: "trash-1", kind: "archive" }]);
    await archiveRepository.listRecycleItems();
    expect(invoke).toHaveBeenLastCalledWith("list_recycle_items");
    await archiveRepository.restoreRecycleItem("trash-1");
    expect(invoke).toHaveBeenLastCalledWith("restore_recycle_item", { itemId: "trash-1" });

    await archiveRepository.openArchiveSources("entry-1");
    expect(invoke).toHaveBeenLastCalledWith("open_entry_sources", { entryId: "entry-1" });

    await archiveRepository.openArchiveStorage("entry-1");
    expect(invoke).toHaveBeenLastCalledWith("open_entry_storage", { entryId: "entry-1" });

    await archiveRepository.updateSnapshotNote("entry-1", "snapshot-1", "进入第二阶段前");
    expect(invoke).toHaveBeenLastCalledWith("update_snapshot_note", {
      entryId: "entry-1",
      snapshotId: "snapshot-1",
      note: "进入第二阶段前",
    });
    await archiveRepository.deleteSnapshot("entry-1", "snapshot-1");
    expect(invoke).toHaveBeenLastCalledWith("delete_snapshot", { entryId: "entry-1", snapshotId: "snapshot-1" });
  });
});
