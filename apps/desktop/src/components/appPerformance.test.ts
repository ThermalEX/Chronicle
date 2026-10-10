import "fake-indexeddb/auto";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
import App from "../App.vue";
import { archiveRepository } from "../services/repository";
import { deviceRepository } from "../services/devices";
import { setLocale } from "../services/i18n";
import type { ArchiveRecord, SnapshotRecord } from "../domain";

function archive(id: string, categoryId?: string): ArchiveRecord {
  return { id, name: id, categoryId, category: "", sourcePath: "C:/synthetic", sources: [], tags: [], kind: "file", storagePolicy: "local", syncMode: "manual", autoBackupEnabled: false, automaticUploadEnabled: false, createdAt: 1, updatedAt: 1, totalBytes: 0 };
}
function snapshot(id: string, archiveId: string): SnapshotRecord {
  return { id, archiveId, title: id, createdAt: 1, totalBytes: 0, contentHash: "hash", files: [], changes: { added: 0, modified: 0, deleted: 0 }, safety: false };
}
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
async function app() {
  let state: any;
  await renderToString(createSSRApp({ ...App, setup(props: unknown, context: unknown) { state = (App as any).setup(props, context); return state; } }));
  return state;
}
beforeEach(() => { vi.stubGlobal("window", { setTimeout: vi.fn(), clearTimeout: vi.fn() }); vi.stubGlobal("localStorage", { getItem: () => null, setItem: vi.fn() }); setLocale("zh-CN"); });
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it("discards an A timeline response after selecting B and B finishes first", async () => {
  const a = deferred<SnapshotRecord[]>(), b = deferred<SnapshotRecord[]>();
  vi.spyOn(archiveRepository, "listSnapshots").mockImplementation(id => id === "a" ? a.promise : b.promise);
  const state = await app(); state.selectedArchiveId.value = "a";
  const first = state.refreshSnapshots("a"); state.selectedArchiveId.value = "b";
  const second = state.refreshSnapshots("b"); b.resolve([snapshot("b1", "b")]); await second;
  a.resolve([snapshot("a1", "a")]); await first;
  expect(state.snapshots.value.map((item: SnapshotRecord) => item.id)).toEqual(["b1"]);
  expect(state.selectedSnapshotId.value).toBe("b1");
});
it("ignores a failed superseded timeline request after the new selection succeeds", async () => {
  let fail!: (cause: unknown) => void;
  const pending = new Promise<SnapshotRecord[]>((_, reject) => { fail = reject; });
  vi.spyOn(archiveRepository, "listSnapshots").mockImplementation(id => id === "a" ? pending : Promise.resolve([snapshot("b1", "b")]));
  const state = await app(); state.selectedArchiveId.value = "a"; const old = state.refreshSnapshots("a");
  state.selectedArchiveId.value = "b"; await state.refreshSnapshots("b"); fail(new Error("Old A failed"));
  await expect(old).resolves.toBeUndefined(); expect(state.snapshots.value[0].id).toBe("b1");
});
it("keeps the inspected snapshot selected when the current timeline refreshes", async () => {
  vi.spyOn(archiveRepository, "listSnapshots").mockResolvedValue([snapshot("a2", "a"), snapshot("a1", "a")]);
  const state = await app(); state.selectedArchiveId.value = "a"; await state.refreshSnapshots("a");
  state.selectedSnapshotId.value = "a1"; await state.refreshSnapshots("a");
  expect(state.selectedSnapshotId.value).toBe("a1");
});
it("shares overlapping requests for the same selected timeline", async () => {
  const pending = deferred<SnapshotRecord[]>();
  const list = vi.spyOn(archiveRepository, "listSnapshots").mockReturnValue(pending.promise);
  const state = await app(); state.selectedArchiveId.value = "a";
  const first = state.refreshSnapshots("a"), second = state.refreshSnapshots("a");
  pending.resolve([snapshot("a1", "a")]); await Promise.all([first, second]);
  expect(list).toHaveBeenCalledTimes(1); expect(state.snapshots.value[0].id).toBe("a1");
});
it("coalesces overlapping capacity refreshes and publishes only the final post-change scan", async () => {
  const old = deferred<{ path: string; totalBytes: number }>();
  const fresh = deferred<{ path: string; totalBytes: number }>();
  const info = vi.spyOn(archiveRepository, "getRepositoryInfo").mockReturnValueOnce(old.promise).mockReturnValueOnce(fresh.promise);
  const state = await app();
  const reads = Array.from({ length: 10 }, () => state.refreshRepositoryInfo());
  expect(info).toHaveBeenCalledTimes(1);
  old.resolve({ path: "old", totalBytes: 1 });
  for (let i = 0; i < 8; i++) await Promise.resolve();
  expect(info).toHaveBeenCalledTimes(2);
  expect(state.repositoryInfo.value.path).not.toBe("old");
  fresh.resolve({ path: "current", totalBytes: 100 }); await Promise.all(reads);
  expect(state.repositoryInfo.value).toEqual({ path: "current", totalBytes: 100 });
  info.mockResolvedValueOnce({ path: "next", totalBytes: 200 });
  await state.refreshRepositoryInfo();
  expect(info).toHaveBeenCalledTimes(3);
  expect(state.repositoryInfo.value.totalBytes).toBe(200);
});
it("retries a superseded capacity failure without reusing a rejected request", async () => {
  let fail!: (cause: unknown) => void;
  const old = new Promise<{ path: string; totalBytes: number }>((_, reject) => { fail = reject; });
  const info = vi.spyOn(archiveRepository, "getRepositoryInfo").mockReturnValueOnce(old).mockResolvedValue({ path: "current", totalBytes: 100 });
  const state = await app(); const first = state.refreshRepositoryInfo(); const second = state.refreshRepositoryInfo();
  fail(new Error("Old scan failed")); await Promise.all([first, second]);
  expect(state.repositoryInfo.value.totalBytes).toBe(100);
  info.mockRejectedValueOnce(new Error("Current scan failed"));
  await expect(state.refreshRepositoryInfo()).rejects.toThrow("Current scan failed");
  await state.refreshRepositoryInfo(); expect(state.repositoryInfo.value.totalBytes).toBe(100);
});
for (const action of ["create", "restore", "sync"] as const) it(`starts a fresh timeline read after ${action} instead of accepting an in-flight pre-write response`, async () => {
  const old = deferred<SnapshotRecord[]>();
  const fresh = snapshot("new", "a");
  vi.spyOn(archiveRepository, "listSnapshots").mockReturnValueOnce(old.promise).mockResolvedValue([fresh]);
  vi.spyOn(archiveRepository, "listArchives").mockResolvedValue([archive("a")]);
  vi.spyOn(archiveRepository, "getRepositoryInfo").mockResolvedValue({ path: "synthetic", totalBytes: 0 });
  vi.spyOn(archiveRepository, "createSnapshot").mockResolvedValue(fresh);
  vi.spyOn(archiveRepository, "restoreSnapshot").mockResolvedValue(undefined);
  const state = await app(); state.archives.value = [archive("a")]; state.selectedArchiveId.value = "a";
  const reading = state.refreshSnapshots("a");
  const mutation = action === "create" ? state.createSnapshot() : action === "restore" ? state.executeRestore(archive("a"), snapshot("old", "a")) : state.refreshAfterSync();
  for (let i = 0; i < 8; i++) await Promise.resolve();
  old.resolve([snapshot("old", "a")]); await Promise.all([reading, mutation]);
  expect(state.snapshots.value.map((item: SnapshotRecord) => item.id)).toEqual(["new"]);
});
it("does not restore a refresh's preferred archive after the user chooses another", async () => {
  const pending = deferred<ArchiveRecord[]>();
  vi.spyOn(archiveRepository, "listArchives").mockReturnValue(pending.promise);
  const state = await app(); state.archives.value = [archive("a"), archive("b")]; state.selectedArchiveId.value = "a";
  const refreshing = state.refreshArchives("a"); state.selectedArchiveId.value = "b";
  pending.resolve([archive("a"), archive("b")]); await refreshing;
  expect(state.selectedArchiveId.value).toBe("b");
});
it("discards an older catalog response and chooses a remaining archive when the current one was deleted", async () => {
  const old = deferred<ArchiveRecord[]>(), latest = deferred<ArchiveRecord[]>();
  vi.spyOn(archiveRepository, "listArchives").mockReturnValueOnce(old.promise).mockReturnValueOnce(latest.promise);
  const state = await app(); state.archives.value = [archive("a"), archive("b")]; state.selectedArchiveId.value = "a";
  const first = state.refreshArchives(), second = state.refreshArchives();
  latest.resolve([archive("b")]); await second;
  old.resolve([archive("a"), archive("b")]); await first;
  expect(state.archives.value.map((item: ArchiveRecord) => item.id)).toEqual(["b"]);
  expect(state.selectedArchiveId.value).toBe("b");
});
it("does not publish a background B timeline while inspecting A", async () => {
  vi.spyOn(archiveRepository, "listSnapshots").mockResolvedValue([snapshot("b1", "b")]);
  const state = await app(); state.selectedArchiveId.value = "a"; state.snapshots.value = [snapshot("a1", "a")]; state.selectedSnapshotId.value = "a1";
  await state.refreshSnapshots("b");
  expect(state.snapshots.value.map((item: SnapshotRecord) => item.id)).toEqual(["a1"]);
  expect(state.selectedArchiveId.value).toBe("a");
});
for (const action of ["sync", "background backup"] as const) it(`invalidates B's nonvisible pre-write request after ${action} before B is selected again`, async () => {
  const old = deferred<SnapshotRecord[]>();
  let bReads = 0;
  vi.spyOn(archiveRepository, "listSnapshots").mockImplementation(id => id === "b" ? ++bReads === 1 ? old.promise : Promise.resolve([snapshot("b-new", "b")]) : Promise.resolve([snapshot("a1", "a")]));
  vi.spyOn(archiveRepository, "listArchives").mockResolvedValue([archive("a"), archive("b")]);
  vi.spyOn(archiveRepository, "getRepositoryInfo").mockResolvedValue({ path: "synthetic", totalBytes: 0 });
  const state = await app(); state.archives.value = [archive("a"), archive("b")]; state.selectedArchiveId.value = "b";
  const readingOld = state.refreshSnapshots("b");
  state.selectedArchiveId.value = "a"; await state.refreshSnapshots("a");
  await (action === "sync" ? state.refreshAfterSync() : state.refreshAfterAutomaticBackup("b"));
  expect(bReads).toBe(1); // Invalidating a hidden timeline must not fetch it in the background.
  state.selectedArchiveId.value = "b";
  const readingFresh = state.refreshSnapshots("b");
  old.resolve([snapshot("b-old", "b")]); await Promise.all([readingOld, readingFresh]);
  expect(bReads).toBe(2);
  expect(state.snapshots.value.map((item: SnapshotRecord) => item.id)).toEqual(["b-new"]);
});
it("finishing an A manual backup does not refresh B again or select A's snapshot while inspecting B", async () => {
  const pending = deferred<SnapshotRecord>();
  vi.spyOn(archiveRepository, "createSnapshot").mockReturnValue(pending.promise);
  vi.spyOn(archiveRepository, "listArchives").mockResolvedValue([archive("a"), archive("b")]);
  vi.spyOn(archiveRepository, "getRepositoryInfo").mockResolvedValue({ path: "synthetic", totalBytes: 0 });
  const list = vi.spyOn(archiveRepository, "listSnapshots").mockImplementation(async id => [snapshot(`${id}1`, id)]);
  const state = await app(); state.archives.value = [archive("a"), archive("b")]; state.selectedArchiveId.value = "a";
  const backingUp = state.createSnapshot();
  state.selectedArchiveId.value = "b"; await state.refreshSnapshots("b");
  list.mockClear();
  pending.resolve(snapshot("a-new", "a")); await backingUp;
  expect(state.selectedArchiveId.value).toBe("b");
  expect(state.selectedSnapshotId.value).toBe("b1");
  expect(list).not.toHaveBeenCalled();
});
it("creates the requested initial snapshot on the new archive even if selection changes during catalog refresh", async () => {
  const listing = deferred<ArchiveRecord[]>();
  const created = archive("new-a");
  vi.spyOn(archiveRepository, "createArchive").mockResolvedValue(created);
  vi.spyOn(archiveRepository, "setArchiveAutomation").mockResolvedValue(undefined);
  vi.spyOn(archiveRepository, "refreshAutoBackup").mockResolvedValue(undefined);
  vi.spyOn(archiveRepository, "getRepositoryInfo").mockResolvedValue({ path: "synthetic", totalBytes: 0 });
  const list = vi.spyOn(archiveRepository, "listArchives").mockReturnValueOnce(listing.promise).mockResolvedValue([created, archive("b")]);
  vi.spyOn(archiveRepository, "listSnapshots").mockImplementation(async id => [snapshot(`${id}1`, id)]);
  const capture = vi.spyOn(archiveRepository, "createSnapshot").mockImplementation(async item => snapshot(`${item.id}-initial`, item.id));
  const state = await app(); state.archives.value = [archive("old"), archive("b")]; state.selectedArchiveId.value = "old";
  const creating = state.createArchive({ name: "New A", sources: [], storagePolicy: "local", syncMode: "manual", autoBackupEnabled: false, automaticUploadEnabled: false, createInitialSnapshot: true });
  for (let i = 0; i < 20; i++) await Promise.resolve();
  expect(list).toHaveBeenCalledTimes(1);
  state.selectedArchiveId.value = "b"; state.selectedSnapshotId.value = "b1";
  listing.resolve([created, archive("b")]); await creating;
  expect(capture.mock.calls[0]?.[0].id).toBe("new-a");
  expect(state.selectedArchiveId.value).toBe("b"); expect(state.selectedSnapshotId.value).toBe("b1");
});
it("appearance-only settings do not read the library, device or storage", async () => {
  const list = vi.spyOn(archiveRepository, "listArchives").mockResolvedValue([]);
  const categories = vi.spyOn(archiveRepository, "listCategories").mockResolvedValue([]);
  const info = vi.spyOn(archiveRepository, "getRepositoryInfo").mockResolvedValue({ path: "synthetic", totalBytes: 0 });
  const device = vi.spyOn(deviceRepository, "read");
  const state = await app(); state.settingsOpen.value = true;
  await state.handleSettingsChanged({ appearance: true, automation: false, device: false, library: false, storage: false }, false);
  expect(list).not.toHaveBeenCalled(); expect(categories).not.toHaveBeenCalled(); expect(info).not.toHaveBeenCalled(); expect(device).not.toHaveBeenCalled();
});
it("uses local device names first, then the newest remote revision, and invalidates on rename", async () => {
  const state = await app();
  state.localDevice.value = { id: "local", name: "This device", revision: 0 };
  state.knownDeviceSources.value = [{ sourceId: "a", sourceName: "A", devices: [{ id: "local", name: "Stale remote", revision: 99 }, { id: "remote", name: "Old", revision: 1 }] }, { sourceId: "b", sourceName: "B", devices: [{ id: "remote", name: "Newest", revision: 2 }] }];
  expect(state.knownDeviceNames.value.get("local")).toBe("This device");
  expect(state.knownDeviceNames.value.get("remote")).toBe("Newest");
  state.localDevice.value.name = "Renamed";
  expect(state.knownDeviceNames.value.get("local")).toBe("Renamed");
});
it("finds nested descendants irrespective of catalog order and counts their archives", async () => {
  const state = await app();
  state.categoryRecords.value = [{ id: "leaf", name: "Leaf", parentId: "middle" }, { id: "middle", name: "Middle", parentId: "root" }, { id: "root", name: "Root" }];
  state.archives.value = [archive("a", "leaf"), archive("b", "middle"), archive("c")]; state.selectedCategoryId.value = "root";
  expect(state.filteredArchives.value.map((item: ArchiveRecord) => item.id)).toEqual(["a", "b"]);
  expect(state.categoryTreeNodes.value.find((item: any) => item.id === "root").count).toBe(2);
});
it("formats a repeated timestamp once, and invalidates its label at midnight or language change", async () => {
  vi.useFakeTimers(); vi.setSystemTime(new Date(2026, 9, 9, 12, 0));
  const state = await app(); const timestamp = new Date(2026, 9, 9, 10, 0).getTime();
  const timeFormat = vi.spyOn(Date.prototype, "toLocaleTimeString"), dateFormat = vi.spyOn(Date.prototype, "toLocaleString");
  const originalFormatter = Intl.DateTimeFormat;
  const formatted = vi.fn((formatter: Intl.DateTimeFormat, value: Date) => formatter.format(value));
  vi.spyOn(Intl, "DateTimeFormat").mockImplementation(function (language, options) {
    const formatter = new originalFormatter(language, options);
    return { format: (value: Date) => formatted(formatter, value) } as Intl.DateTimeFormat;
  });
  const first = state.formatTime(timestamp); for (let i = 0; i < 9; i++) expect(state.formatTime(timestamp)).toBe(first);
  expect(first).toContain("今天"); expect(timeFormat.mock.calls.length + dateFormat.mock.calls.length + formatted.mock.calls.length).toBe(1);
  vi.setSystemTime(new Date(2026, 9, 10, 0, 0)); expect(state.formatTime(timestamp)).not.toContain("今天");
  setLocale("en"); expect(state.formatTime(timestamp)).toMatch(/Oct/);
  vi.useRealTimers();
});
