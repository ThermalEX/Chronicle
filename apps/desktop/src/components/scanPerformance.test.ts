import "fake-indexeddb/auto";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
import SteamScanSettings from "./SteamScanSettings.vue";
import GalgameScanSettings from "./GalgameScanSettings.vue";
import type { ArchiveRecord, ArchiveSource } from "../domain";

beforeEach(() => vi.stubGlobal("localStorage", { getItem: () => null, setItem: vi.fn() }));
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
async function scan(component: any) {
  let state: any;
  await renderToString(createSSRApp({ ...component, setup(props: unknown, context: unknown) { state = component.setup(props, context); return state; } }));
  return state;
}
function archive(sources: ArchiveSource[]): ArchiveRecord {
  return { id: "a", name: "Synthetic", sources, sourcePath: "", category: "", tags: [], kind: "collection", storagePolicy: "local", syncMode: "manual", autoBackupEnabled: false, automaticUploadEnabled: false, createdAt: 0, updatedAt: 0, totalBytes: 0 };
}
for (const [name, component] of [["Steam", SteamScanSettings], ["Galgame", GalgameScanSettings]] as const) {
  it(`${name} normalizes existing managed sources once across repeated row checks`, async () => {
    let reads = 0;
    const source: ArchiveSource = { id: "source", name: "Save", kind: "folder", get path() { reads++; return "C:\\Games\\Save"; } };
    const state = await scan(component); state.archives.value = [archive([source])];
    for (let index = 0; index < 100; index++) expect(state.managed({ kind: "file", path: "c:/games/save/slot.dat" })).toBe(true);
    expect(reads).toBe(1);
    state.archives.value = [];
    expect(state.managed({ kind: "file", path: "c:/games/save/slot.dat" })).toBe(false);
  });
  it(`${name} preserves registry namespaces, exact files and folder boundaries`, async () => {
    const state = await scan(component);
    state.archives.value = [archive([
      { id: "folder", name: "Folder", path: "C:\\Save\\", kind: "folder" },
      { id: "file", name: "File", path: "C:/single.sav", kind: "file" },
      { id: "reg", name: "Registry", path: "HKEY_CURRENT_USER\\Software\\Game", kind: "registry" },
    ])];
    expect(state.managed({ kind: "file", path: "c:/save/slot.sav" })).toBe(true);
    expect(state.managed({ kind: "folder", path: "C:/SAVE" })).toBe(true);
    expect(state.managed({ kind: "file", path: "C:/save-other/slot.sav" })).toBe(false);
    expect(state.managed({ kind: "file", path: "C:/single.sav/child" })).toBe(false);
    expect(state.managed({ kind: "registry", path: "hkey_current_user/software/game/slot" })).toBe(true);
    expect(state.managed({ kind: "file", path: "hkey_current_user/software/game/slot" })).toBe(false);
  });
}
it("Steam selection still counts separate accounts and removes only the visible selections", async () => {
  const state = await scan(SteamScanSettings);
  state.result.value = { steamPath: "", scannedAt: 0, databaseUpdatedAt: 0, libraries: [], warnings: [], games: [
    { appId: "1", name: "Game", installPath: "C:/Game", hasRules: true, sources: [
      { kind: "file", path: "C:/u1.sav", user: { accountId: "1", displayName: "User" } },
      { kind: "file", path: "C:/u2.sav", user: { accountId: "2", displayName: "User" } },
    ] },
    { appId: "2", name: "Other", installPath: "C:/Other", hasRules: true, sources: [{ kind: "file", path: "C:/other.sav" }] },
  ] };
  state.selectAll(); expect(state.selectedArchives.value).toBe(3);
  state.search.value = "Game"; state.selectAll();
  expect(state.selected.value).toEqual(["2:file:c:/other.sav"]); expect(state.selectedArchives.value).toBe(1);
});
