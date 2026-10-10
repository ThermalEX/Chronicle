import "fake-indexeddb/auto";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createSSRApp, nextTick } from "vue";
import { renderToString } from "@vue/server-renderer";
vi.mock("../services/themeSounds", async (original) => ({ ...await original<typeof import("../services/themeSounds")>(), playThemeSound: vi.fn(async () => true) }));
import App from "../App.vue";
import { playThemeSound } from "../services/themeSounds";
import { cloudSettings } from "../services/settings";
import { newCloudSource } from "../services/cloudSourceControls";
import { cloudRepository } from "../services/cloud";
import { archiveRepository } from "../services/repository";
import {createTheme,savedPersonalization,activeAppearance} from "../services/personalization";
import * as appearance from "../services/appearance";
import type { ArchiveRecord, SnapshotRecord } from "../domain";

const originalCloud = { ...cloudSettings, sources: [...cloudSettings.sources] };
beforeEach(() => {
  vi.stubGlobal("window", { setTimeout: vi.fn(), clearTimeout: vi.fn() });
  vi.mocked(playThemeSound).mockClear();
  cloudSettings.enabled = true;
  cloudSettings.sources = [newCloudSource("legacy_webdav", "first", 1), newCloudSource("legacy_webdav", "second", 2)];
});
afterEach(() => { Object.assign(cloudSettings, originalCloud); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

async function app() {
  let state: any;
  await renderToString(createSSRApp({ ...App, setup(props: unknown, context: unknown) {
    state = (App as any).setup(props, context);
    return state;
  } }));
  return state;
}

it("keeps saved solid colors after removing the custom theme instead of reapplying legacy settings",async()=>{
  vi.stubGlobal("localStorage",{getItem:()=>null,setItem:vi.fn()});
  savedPersonalization.value={draft:{formatVersion:1,mode:"solid",solid:{colorTheme:"indigo",colorMode:"dark"},selectedThemeId:null,themes:[]},warnings:[]};
  const animate=vi.spyOn(appearance,"animateAppearance").mockResolvedValue(undefined);
  vi.spyOn(archiveRepository,"listArchives").mockResolvedValue([]);
  vi.spyOn(archiveRepository,"listCategories").mockResolvedValue([]);
  vi.spyOn(archiveRepository,"getRepositoryInfo").mockResolvedValue({path:"test",totalBytes:0});
  const state=await app();await state.handleSettingsChanged(false);
  expect(animate).toHaveBeenLastCalledWith({colorTheme:"indigo",colorMode:"dark"});
});
it("does not overwrite an open settings preview after a partially failed save",async()=>{
  vi.stubGlobal("localStorage",{getItem:()=>null,setItem:vi.fn()});
  const animate=vi.spyOn(appearance,"animateAppearance").mockResolvedValue(undefined);
  vi.spyOn(archiveRepository,"listArchives").mockResolvedValue([]);
  vi.spyOn(archiveRepository,"listCategories").mockResolvedValue([]);
  vi.spyOn(archiveRepository,"getRepositoryInfo").mockResolvedValue({path:"test",totalBytes:0});
  const state=await app();state.settingsOpen.value=true;await state.handleSettingsChanged(false);
  expect(animate).not.toHaveBeenCalled();
});

it("reports one connection sound after all cloud sources finish, without one sound per source", async () => {
  const check = vi.spyOn(cloudRepository, "test").mockResolvedValue(undefined);
  const state = await app();
  await state.checkCloudSources();
  expect(check).toHaveBeenCalledTimes(2);
  expect(playThemeSound).toHaveBeenCalledExactlyOnceWith("connected");
});

it("uses the failed-connection sound if any cloud source fails, and stays silent with cloud disabled", async () => {
  vi.spyOn(cloudRepository, "test").mockResolvedValueOnce(undefined).mockRejectedValueOnce(new Error("Offline"));
  const state = await app();
  await state.checkCloudSources();
  expect(playThemeSound).toHaveBeenCalledExactlyOnceWith("connectionFailed");
  vi.mocked(playThemeSound).mockClear();
  cloudSettings.enabled = false;
  await state.checkCloudSources();
  expect(playThemeSound).not.toHaveBeenCalled();
});

it("distinguishes ordinary notices from operation completion without translated-text matching", async () => {
  const state = await app();
  state.showNotice("arbitrary ordinary message", "info");
  expect(playThemeSound).toHaveBeenLastCalledWith("default");
  state.showNotice("arbitrary completed message", "success", "notification");
  expect(playThemeSound).toHaveBeenLastCalledWith("notification");
});

it("does not announce a successful connection after cloud sync is disabled mid-check", async () => {
  let complete!: () => void;
  vi.spyOn(cloudRepository, "test").mockImplementation(() => new Promise<void>((resolve) => { complete = resolve; }));
  cloudSettings.sources = cloudSettings.sources.slice(0, 1);
  const state = await app();
  const checking = state.checkCloudSources();
  cloudSettings.enabled = false;
  await nextTick();
  complete();
  await checking;
  expect(playThemeSound).not.toHaveBeenCalledWith("connected");
});

it("plays a completion notification only after backup and restore finish", async () => {
  const archive: ArchiveRecord = { id: "a", name: "test", sourcePath: "C:/test", sources: [], category: "", tags: [], kind: "file",
    storagePolicy: "local", syncMode: "manual", autoBackupEnabled: false, automaticUploadEnabled: false, createdAt: 1, updatedAt: 1, totalBytes: 0 };
  const snapshot: SnapshotRecord = { id: "s", archiveId: "a", title: "test", createdAt: 1, totalBytes: 0, contentHash: "hash", files: [], changes: { added: 0, modified: 0, deleted: 0 }, safety: false };
  vi.spyOn(archiveRepository, "listArchives").mockResolvedValue([archive]);
  vi.spyOn(archiveRepository, "listSnapshots").mockResolvedValue([snapshot]);
  vi.spyOn(archiveRepository, "getRepositoryInfo").mockResolvedValue({ path: "test", totalBytes: 0 });
  const backup = vi.spyOn(archiveRepository, "createSnapshot").mockResolvedValue(snapshot);
  const restore = vi.spyOn(archiveRepository, "restoreSnapshot").mockResolvedValue(undefined);
  const state = await app();
  state.archives.value = [archive];
  state.selectedArchiveId.value = "a";
  await state.createSnapshot();
  expect(backup).toHaveBeenCalledTimes(1);
  expect(playThemeSound).toHaveBeenCalledExactlyOnceWith("notification");
  vi.mocked(playThemeSound).mockClear();
  await state.executeRestore(archive, snapshot);
  expect(restore).toHaveBeenCalledTimes(1);
  expect(playThemeSound).toHaveBeenCalledExactlyOnceWith("notification");
  vi.mocked(playThemeSound).mockClear();
  restore.mockRejectedValueOnce(new Error("Restore failed"));
  await state.executeRestore(archive, snapshot);
  expect(playThemeSound).toHaveBeenCalledExactlyOnceWith("default");
  expect(state.busyAction.value).toBeUndefined();
});
it("persists the active local theme mode and accent without overwriting solid colors",async()=>{
  const theme=createTheme("牧濑红莉栖");theme.colorTheme="custom";theme.customAccent="#b83e49";theme.colorMode="light";
  savedPersonalization.value={draft:{formatVersion:1,mode:"custom",solid:{colorTheme:"indigo",colorMode:"system"},selectedThemeId:theme.id,themes:[theme]},warnings:[]};
  const state=await app();await state.persistActiveAppearance({colorTheme:"custom",customAccent:"#b83e49",colorMode:"dark"});
  expect(activeAppearance()).toMatchObject({colorTheme:"custom",customAccent:"#b83e49",colorMode:"dark"});
  expect(savedPersonalization.value.draft.solid).toEqual({colorTheme:"indigo",colorMode:"system"});
  savedPersonalization.value.draft.mode="solid";
  await state.persistActiveAppearance({colorTheme:"rose",colorMode:"light"});
  expect(activeAppearance()).toMatchObject({colorTheme:"rose",colorMode:"light"});
  expect(savedPersonalization.value.draft.themes[0]!.colorMode).toBe("dark");
});
