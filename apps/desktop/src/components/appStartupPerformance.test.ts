import "fake-indexeddb/auto";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
const hooks = vi.hoisted(() => ({ mounted: [] as Array<() => Promise<void>>, events: new Map<string, (event: any) => Promise<void>>() }));
vi.mock("vue", async original => ({ ...await original<typeof import("vue")>(), onMounted: (callback: () => Promise<void>) => hooks.mounted.push(callback) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (name, callback) => { hooks.events.set(name, callback); return () => hooks.events.delete(name); }) }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }) }));
vi.mock("../services/repository", async original => ({ ...await original<typeof import("../services/repository")>(), isTauriRuntime: true }));
vi.mock("../services/backupAutomation", async original => ({ ...await original<typeof import("../services/backupAutomation")>(), getBackupRuntimeStates: async () => [], subscribeBackupRuntime: async () => () => {} }));
import { createSSRApp, nextTick } from "vue";
import { renderToString } from "@vue/server-renderer";
import App from "../App.vue";
import { archiveRepository } from "../services/repository";
import { deviceRepository } from "../services/devices";
import * as settings from "../services/settings";
import * as personalization from "../services/personalization";
import { snapshotSync, type SyncPlan } from "../services/snapshotSync";
import type { ArchiveRecord, SnapshotRecord } from "../domain";

function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
const archive = (id: string): ArchiveRecord => ({ id, name: id, sources: [], sourcePath: "C:/synthetic", category: "", tags: [], kind: "file", storagePolicy: "local", syncMode: "manual", autoBackupEnabled: false, automaticUploadEnabled: false, createdAt: 0, updatedAt: 0, totalBytes: 0 });
const snapshot = (id: string, archiveId: string): SnapshotRecord => ({ id, archiveId, title: id, createdAt: 1, totalBytes: 0, contentHash: "hash", files: [], changes: { added: 0, modified: 0, deleted: 0 }, safety: false });
async function flush() { for (let index = 0; index < 15; index++) await Promise.resolve(); await nextTick(); }
async function app() {
  let state: any;
  const html = await renderToString(createSSRApp({ ...App, setup(p: unknown, c: unknown) { state = (App as any).setup(p, c); return state; } }));
  return { state, html, render: () => renderToString(createSSRApp({ ...App, setup: () => state })) };
}
beforeEach(() => {
  hooks.mounted = []; hooks.events.clear();
  vi.stubGlobal("window", { setTimeout: vi.fn(), clearTimeout: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }) });
  vi.stubGlobal("document", { hidden: false, addEventListener: vi.fn(), removeEventListener: vi.fn(), documentElement: { dataset: {}, style: { setProperty: vi.fn() } } });
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: vi.fn() });
  vi.spyOn(settings, "initializeSettings").mockImplementation(async () => { settings.appSettings.colorTheme = "indigo"; settings.appSettings.checkForUpdates = false; settings.appSettings.checkCloudOnLaunch = false; });
  vi.spyOn(personalization, "loadPersonalization").mockImplementation(async () => { personalization.savedPersonalization.value = { draft: { formatVersion: 1, mode: "solid", solid: { colorTheme: "rose", colorMode: "dark" }, themes: [] }, warnings: [] }; });
  vi.spyOn(personalization, "applyPersonalizationRuntime").mockResolvedValue(undefined);
  vi.spyOn(archiveRepository, "listArchives").mockResolvedValue([archive("a"), archive("b")]);
  vi.spyOn(archiveRepository, "listCategories").mockResolvedValue([]);
  vi.spyOn(archiveRepository, "listSnapshots").mockImplementation(async id => [snapshot(`${id}1`, id)]);
  vi.spyOn(archiveRepository, "getRepositoryInfo").mockResolvedValue({ path: "synthetic", totalBytes: 0 });
  vi.spyOn(archiveRepository, "refreshAutoBackup").mockResolvedValue(undefined);
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
it("accumulates selected quick-sync result deltas across sources without regressing or double counting", async () => {
  const pending = deferred<[]>();
  const plan: SyncPlan = { id: "request", sources: ["a", "b"].map(sourceId => ({ sourceId, sourceName: sourceId, error: null, upgradeRequired: false, archiveNames: {}, devices: [], operations: [{ id: sourceId, action: "upload", selected: true, reason: null, deletion: null, snapshot: { id: sourceId, entry_id: "a", title: sourceId, created_at_ms: 1, size_bytes: 1 } }] })) };
  let requestId = "";
  vi.spyOn(snapshotSync, "preview").mockImplementation(async id => { requestId = id; return { ...plan, id }; });
  vi.spyOn(snapshotSync, "apply").mockReturnValue(pending.promise);
  settings.appSettings.showSyncDetails = false;
  const { state } = await app();
  const syncing = state.startSnapshotSync({ archives: [], sourceIds: ["a", "b"], allArchives: true });
  await flush();
  const progress = hooks.events.get("snapshot-sync-progress")!;
  const first = { operationId: "a", sourceId: "a", status: "success", error: null };
  await progress({ payload: { requestId, results: [first] } });
  expect(state.syncProgress.value).toEqual({ current: 1, total: 2 });
  await progress({ payload: { requestId, results: [{ ...first, operationId: "b", sourceId: "b" }] } });
  expect(state.syncProgress.value.current).toBe(2);
  await progress({ payload: { requestId, results: [first], sourcesChecked: 1 } });
  await progress({ payload: { requestId, results: [{ ...first, operationId: "records" }] } });
  await progress({ payload: { requestId: "other", results: [] } });
  await progress({ payload: { requestId, sourcesChecked: 1 } });
  expect(state.syncProgress.value.current).toBe(2);
  pending.resolve([]); await syncing;
});
it("withholds the main shell until saved theme metadata has been applied", async () => {
  const initial = await app();
  expect(initial.html).not.toContain('class="app-shell"');
});
it("applies the saved accent and reveals the shell while device and media loads are pending", async () => {
  const device = deferred<{ id: string; name: string; revision: number }>(), metadata = deferred<void>(), media = deferred<void>();
  vi.spyOn(deviceRepository, "read").mockReturnValue(device.promise);
  vi.mocked(personalization.loadPersonalization).mockImplementation(async () => { await metadata.promise; personalization.savedPersonalization.value = { draft: { formatVersion: 1, mode: "solid", solid: { colorTheme: "custom", customAccent: "#b83e49", colorMode: "dark" }, themes: [] }, warnings: [] }; });
  vi.mocked(personalization.applyPersonalizationRuntime).mockReturnValue(media.promise);
  const { render } = await app(); const starting = hooks.mounted[0]!(); await flush();
  metadata.resolve(); await flush();
  expect(await render()).toContain('class="app-shell"');
  expect(document.documentElement.dataset.colorTheme).toBe("custom");
  expect(document.documentElement.style.setProperty).toHaveBeenCalledWith("--custom-accent", "#b83e49");
  device.resolve({ id: "device", name: "Test", revision: 1 }); media.resolve(); await starting;
});
it("background backup B preserves A selection and only refreshes the visible timeline", async () => {
  const { state } = await app(); await hooks.mounted[0]!(); await flush();
  state.selectedArchiveId.value = "a"; await state.refreshSnapshots("a"); state.selectedSnapshotId.value = "a1";
  vi.mocked(archiveRepository.listSnapshots).mockClear();
  await hooks.events.get("auto-backup-created")!({ payload: { archiveId: "b" } });
  expect(state.selectedArchiveId.value).toBe("a"); expect(state.selectedSnapshotId.value).toBe("a1");
  expect(state.snapshots.value.map((item: SnapshotRecord) => item.archiveId)).toEqual(["a"]);
  expect(archiveRepository.listSnapshots).not.toHaveBeenCalled();
});
it("coalesces simultaneous background backups into one library and storage refresh", async () => {
  const { state } = await app(); await hooks.mounted[0]!(); await flush();
  state.selectedArchiveId.value = "a"; await state.refreshSnapshots("a");
  vi.mocked(archiveRepository.listArchives).mockClear(); vi.mocked(archiveRepository.getRepositoryInfo).mockClear(); vi.mocked(archiveRepository.listSnapshots).mockClear();
  const handler = hooks.events.get("auto-backup-created")!;
  await Promise.all([handler({ payload: { archiveId: "a" } }), handler({ payload: { archiveId: "b" } })]);
  expect(archiveRepository.listArchives).toHaveBeenCalledTimes(1); expect(archiveRepository.getRepositoryInfo).toHaveBeenCalledTimes(1);
  expect(archiveRepository.listSnapshots).toHaveBeenCalledExactlyOnceWith("a");
});
it("automation-only settings restart automation without rereading unrelated library state", async () => {
  const { state } = await app(); await hooks.mounted[0]!(); await flush();
  vi.mocked(archiveRepository.refreshAutoBackup).mockClear(); vi.mocked(archiveRepository.listArchives).mockClear(); vi.mocked(archiveRepository.listCategories).mockClear(); vi.mocked(archiveRepository.getRepositoryInfo).mockClear();
  await state.handleSettingsChanged({ automation: true, appearance: false, device: false, library: false, storage: false }, false);
  expect(archiveRepository.refreshAutoBackup).toHaveBeenCalledTimes(1); expect(archiveRepository.listArchives).not.toHaveBeenCalled(); expect(archiveRepository.listCategories).not.toHaveBeenCalled(); expect(archiveRepository.getRepositoryInfo).not.toHaveBeenCalled();
});
