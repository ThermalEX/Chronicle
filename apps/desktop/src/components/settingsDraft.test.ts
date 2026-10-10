import "fake-indexeddb/auto";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
vi.mock("@tauri-apps/api/core", async (original) => ({ ...await original<typeof import("@tauri-apps/api/core")>(), isTauri: vi.fn(() => false), invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
import SettingsDialog from "./SettingsDialog.vue";
import { appSettings, resetAppSettings } from "../services/settings";
import * as settings from "../services/settings";
import { deviceRepository } from "../services/devices";
import { locale, setLocale } from "../services/i18n";
import { archiveRepository } from "../services/repository";
import * as wallpaper from "../services/wallpaper";
import * as core from "@tauri-apps/api/core";
import * as fileDialog from "@tauri-apps/plugin-dialog";
import {savedPersonalization,createTheme} from "../services/personalization";
import * as personalization from "../services/personalization";
import { localSounds } from "../services/themeSounds";

async function dialog(configure?: (state: any) => void) {
  let state: any;
  let closed = 0;
  const previewStates: boolean[] = [];
  const savedEvents: unknown[] = [];
  const changedEvents: unknown[] = [];
  const component = { ...SettingsDialog, setup(props: unknown, context: unknown) {
    state = (SettingsDialog as any).setup(props, context);
    configure?.(state);
    return state;
  } };
  const html = await renderToString(createSSRApp(component, { onClose: () => closed++, onWallpaperPreview: (active: boolean) => previewStates.push(active), onSaved: (changes: unknown) => savedEvents.push(changes), onChanged: (changes: unknown) => changedEvents.push(changes) }));
  return { state, html, closed: () => closed, previewStates, savedEvents, changedEvents };
}

beforeEach(() => {
  vi.mocked(core.isTauri).mockReturnValue(false);
  vi.mocked(core.invoke).mockReset();
  vi.mocked(fileDialog.open).mockReset();
  wallpaper.currentWallpaper.value = { mode: "color", transparency: 28, blurPx: 12 };
  localSounds.value = { enabled: false, volume: 60, files: {} };
  const values = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
    removeItem: (key: string) => { values.delete(key); },
  });
  resetAppSettings();
  const theme=createTheme("牧濑红莉栖");
  savedPersonalization.value={draft:{formatVersion:1,mode:"custom",solid:{colorTheme:"teal",colorMode:"system"},selectedThemeId:theme.id,themes:[theme]},warnings:[]};
  setLocale("zh-CN");
});

afterEach(async () => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  await new Promise<void>((resolve, reject) => {
    const request = indexedDB.deleteDatabase("chronicle-local");
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
});

describe("settings drafts", () => {
  it("saves only personalization for a color edit and reports no backup or library refresh", async () => {
    const persist = vi.spyOn(settings, "saveAppSettings");
    const {state, savedEvents, closed} = await dialog();
    state.personalizationDraft.value.themes[0].colorTheme = "indigo";
    await state.save();
    expect(persist).not.toHaveBeenCalled();
    expect(savedEvents).toEqual([{automation:false,device:false,library:false,storage:false,appearance:true}]);
    expect(closed()).toBe(1);
  });
  it("reports only successfully committed scopes when a later personalization save fails", async () => {
    vi.spyOn(personalization, "savePersonalization").mockRejectedValueOnce(new Error("Disk full"));
    const {state, changedEvents} = await dialog();
    state.draft.autoBackupDelaySeconds = 17;
    state.personalizationDraft.value.themes[0].colorTheme = "indigo";
    await state.save();
    expect(changedEvents).toEqual([{automation:true,device:false,library:false,storage:false,appearance:false}]);
  });
  it("does not load hidden sections until first visit and retains automation edits on revisit", async () => {
    const automation = vi.spyOn(archiveRepository, "listArchives").mockResolvedValue([]);
    const device = vi.spyOn(deviceRepository, "read");
    const {state, html} = await dialog();
    expect(html).not.toContain('data-testid="theme-library"');
    await state.loadSection("software");
    expect(automation).not.toHaveBeenCalled();
    expect(device).not.toHaveBeenCalled();
    await state.loadSection("backup");
    await state.loadSection("backup");
    expect(automation).toHaveBeenCalledTimes(1);
    await state.loadSection("device");
    await state.loadSection("device");
    expect(device).toHaveBeenCalledTimes(1);
  });
  it("retries a failed section visit without reloading a successful edited section", async () => {
    vi.spyOn(deviceRepository, "read").mockRejectedValueOnce(new Error("Busy"))
      .mockResolvedValue({id:"local-id",name:"My device",revision:0});
    const {state} = await dialog();
    await state.loadSection("device");
    expect(state.localDevice.value).toBeUndefined();
    await state.loadSection("device");
    expect(state.deviceName.value).toBe("My device");
    state.deviceName.value = "Unsaved name";
    await state.loadSection("device");
    expect(state.deviceName.value).toBe("Unsaved name");
  });
  it("rejects file drops behind close and action confirmation dialogs", async () => {
    const {state}=await dialog();state.activeSection.value="personalization";
    const acceptFileDrop=vi.fn(async()=>true);state.personalizationEditor.value={acceptFileDrop};
    state.closeConfirmOpen.value=true;
    expect(await state.acceptFileDrop(["theme.zip"],{x:100,y:100})).toBe(false);
    state.closeConfirmOpen.value=false;state.confirmAction.value="reset";
    expect(await state.acceptFileDrop(["theme.zip"],{x:100,y:100})).toBe(false);
    expect(acceptFileDrop).not.toHaveBeenCalled();
  });
  it("stages sounds separately and asks before discarding them", async () => {
    const {state,closed}=await dialog();
    const theme=state.personalizationDraft.value.themes[0];
    Object.assign(theme.sounds,{enabled:true,volume:25});
    state.requestClose();
    expect(state.closeConfirmOpen.value).toBe(true);
    expect(savedPersonalization.value.draft.themes[0]!.sounds.enabled).toBe(false);
    state.discardClose();
    expect(closed()).toBe(1);
    expect(core.invoke).not.toHaveBeenCalled();
  });
  it("keeps failed theme saves editable and retries only unsaved personalization",async()=>{
    const persist=vi.spyOn(personalization,"savePersonalization").mockRejectedValueOnce(new Error("Disk full"));
    const {state,closed}=await dialog();
    state.draft.autoBackupDelaySeconds=17;
    state.personalizationDraft.value.themes[0].sounds.volume=30;
    await state.save();
    expect(closed()).toBe(0);
    expect(appSettings.autoBackupDelaySeconds).toBe(17);
    expect(state.saveError.value).toContain("部分设置已保存");
    expect(state.hasChanges.value).toBe(true);
    await state.save();
    expect(persist).toHaveBeenCalledTimes(2);
    expect(savedPersonalization.value.draft.themes[0]!.sounds.volume).toBe(30);
    expect(state.hasChanges.value).toBe(false);
    expect(closed()).toBe(1);
  });
  it("rejects an invalid custom color without saving any settings",async()=>{
    const {state,closed}=await dialog();
    state.personalizationDraft.value.themes[0].colorTheme="custom";
    state.personalizationDraft.value.themes[0].customAccent="nothex";
    state.draft.autoBackupDelaySeconds=17;
    await state.save();
    expect(appSettings.autoBackupDelaySeconds).toBe(5);
    expect(state.saveError.value).not.toBe("");
    expect(closed()).toBe(0);
  });
  it("shows a custom color picker and retains it as an unsaved draft",async()=>{
    const {html,state}=await dialog(state=>{state.activeSection.value="personalization";state.personalizationDraft.value.themes[0].colorTheme="custom";state.personalizationDraft.value.themes[0].customAccent="#b83e49";});
    expect(html).toContain('type="color"');expect(html).toContain("点击选择或拖入主题包");expect(html).toContain("应用图标");
    expect(appSettings.colorTheme).toBe("teal");state.requestClose();expect(state.closeConfirmOpen.value).toBe(true);
  });
  it("separates personalization from application and behavior cards",async()=>{
    const {html}=await dialog();
    const software=html.split('aria-labelledby="software-title"')[1]!.split("</main>")[0]!;
    expect(software).toContain("界面语言");expect(software).toContain("随系统启动");
    expect(software).not.toContain("背景图片");expect(software).not.toContain("导入主题");
    expect(html).not.toContain('data-testid="theme-library"');expect(html).toContain("保存设置");
    const personalizationView=await dialog(state=>{state.activeSection.value="personalization";});
    expect(personalizationView.html).toContain('data-testid="theme-library"');
  });
  it("returns from the actual app preview without discarding or saving the draft",async()=>{
    const {state,previewStates,closed}=await dialog();
    vi.stubGlobal("document",{querySelector:()=>null});
    state.personalizationDraft.value.themes[0].transparency=40;
    state.showWallpaperPreview(true);state.showWallpaperPreview(false);
    expect(previewStates).toEqual([true,false]);
    expect(state.personalizationDraft.value.themes[0].transparency).toBe(40);
    expect(state.hasChanges.value).toBe(true);expect(closed()).toBe(0);
  });
  it("restores the saved wallpaper style when changes are discarded",async()=>{
    const apply=vi.spyOn(wallpaper,"applyWallpaper");const {state}=await dialog();
    state.personalizationDraft.value.themes[0].transparency=35;
    state.discardClose();
    expect(apply).toHaveBeenLastCalledWith(expect.objectContaining({transparency:28}));
  });
  it("allows an empty custom theme to use its color without a wallpaper",async()=>{
    const {state,closed}=await dialog();await state.save();expect(closed()).toBe(1);expect(wallpaper.currentWallpaper.value.mode).toBe("color");
  });
  it("blocks saving and closing while a theme resource operation is running",async()=>{
    const {state,closed}=await dialog();state.themeBusy.value=true;state.draft.autoBackupDelaySeconds=17;
    state.requestClose();await state.save();
    expect(closed()).toBe(0);expect(appSettings.autoBackupDelaySeconds).toBe(5);
  });
  it("keeps sync details changes pending until Save settings", async () => {
    const { state } = await dialog();
    state.draft.showSyncDetails = true;
    expect(appSettings.showSyncDetails).toBe(false);
    state.requestClose();
    expect(state.closeConfirmOpen.value).toBe(true);
    await state.save();
    expect(appSettings.showSyncDetails).toBe(true);
  });
  it("distinguishes the device and backup navigation icons and removes the extra save row", async () => {
    const { html } = await dialog((state) => {
      state.activeSection.value = "device";
      state.localDevice.value = { id: "device-a", name: "PC", revision: 0 };
      state.deviceName.value = "PC";
    });
    expect(html).toMatch(/lucide-monitor[\s\S]*?<span[^>]*>设备<\/span>/);
    expect(html).toMatch(/lucide-hard-drive[\s\S]*?<span[^>]*>存储与备份<\/span>/);
    expect(html).not.toContain("保存本机名称");
    expect(html).toContain("保存设置");
  });

  it("provides save, discard and keep-editing choices in English", async () => {
    setLocale("en");
    const { html } = await dialog((state) => { state.closeConfirmOpen.value = true; });
    expect(html).toContain("Save and close");
    expect(html).toContain("Keep editing");
    expect(html).toContain("Discard");
  });

  it("asks before closing modified settings, and discards without saving", async () => {
    const { state, closed } = await dialog();
    state.draft.autoBackupDelaySeconds = 17;
    state.requestClose();
    expect(closed()).toBe(0);
    expect(state.closeConfirmOpen.value).toBe(true);
    state.discardClose();
    expect(closed()).toBe(1);
    expect(appSettings.autoBackupDelaySeconds).toBe(5);
  });

  it("closes unchanged settings directly", async () => {
    const { state, closed } = await dialog();
    state.requestClose();
    expect(closed()).toBe(1);
  });

  it("does not prompt after a setting is changed back to its saved value", async () => {
    const { state, closed } = await dialog();
    state.draft.autoBackupDelaySeconds = 17;
    state.draft.autoBackupDelaySeconds = 5;
    state.requestClose();
    expect(closed()).toBe(1);
  });

  it("Escape keeps editing when the unsaved-change confirmation is open", async () => {
    const { state, closed } = await dialog();
    state.draft.autoBackupDelaySeconds = 17;
    state.requestClose();
    state.onEscape({ target: { closest: () => null } });
    expect(state.closeConfirmOpen.value).toBe(false);
    expect(state.draft.autoBackupDelaySeconds).toBe(17);
    expect(closed()).toBe(0);
  });

  it("cancelling a tutorial restart does not restart it after a later normal save", async () => {
    const { state, closed } = await dialog();
    state.draft.autoBackupDelaySeconds = 17;
    state.requestClose("restart-tutorial");
    state.continueEditing();
    await state.save();
    expect(closed()).toBe(1);
  });

  it("previews language without persisting it and restores it on discard", async () => {
    const { state } = await dialog();
    await state.updateLanguage("en");
    expect(locale.value).toBe("en");
    expect(appSettings.language).toBe("zh-CN");
    state.discardClose();
    expect(locale.value).toBe("zh-CN");
  });

  it("saves the device name and application settings through the same action", async () => {
    const { state, closed } = await dialog();
    await state.loadDevice();
    state.deviceName.value = "  Gaming PC  ";
    state.draft.autoBackupDelaySeconds = 17;
    state.activeSection.value = "backup";
    await state.save();
    expect((await deviceRepository.read()).name).toBe("Gaming PC");
    expect(appSettings.autoBackupDelaySeconds).toBe(17);
    expect(closed()).toBe(1);
  });

  it("rejects invalid device names before saving any other setting", async () => {
    const { state, closed } = await dialog();
    await state.loadDevice();
    state.deviceName.value = " ";
    state.draft.autoBackupDelaySeconds = 17;
    await state.save();
    expect(closed()).toBe(0);
    expect(appSettings.autoBackupDelaySeconds).toBe(5);
    expect(state.saveError.value).not.toBe("");
  });

  it("keeps unsaved changes and the dialog open when storage fails", async () => {
    const { state, closed } = await dialog();
    state.draft.autoBackupDelaySeconds = 17;
    vi.spyOn(localStorage, "setItem").mockImplementation(() => { throw new Error("Disk full"); });
    await state.save();
    expect(closed()).toBe(0);
    expect(appSettings.autoBackupDelaySeconds).toBe(5);
    expect(state.draft.autoBackupDelaySeconds).toBe(17);
    expect(state.saveError.value).toContain("Disk full");
  });

  it("resetting changes the draft rather than the saved settings", async () => {
    appSettings.autoBackupDelaySeconds = 17;
    const { state } = await dialog();
    state.reset();
    expect(state.draft.autoBackupDelaySeconds).toBe(5);
    expect(appSettings.autoBackupDelaySeconds).toBe(17);
  });

  it("stages bulk automation and persists it only through Save settings", async () => {
    await archiveRepository.createArchive({ name: "Game", sources: [], storagePolicy: "local", syncMode: "manual", autoBackupEnabled: false, automaticUploadEnabled: false, createInitialSnapshot: false });
    const { state, closed } = await dialog();
    await state.loadAutomation();
    await state.toggleAllAutomation("backup");
    await state.toggleAllAutomation("upload");
    expect((await archiveRepository.listArchives())[0]).toMatchObject({ autoBackupEnabled: false, automaticUploadEnabled: false });
    state.requestClose();
    expect(closed()).toBe(0);
    expect(state.closeConfirmOpen.value).toBe(true);
    await state.save();
    expect((await archiveRepository.listArchives())[0]).toMatchObject({ autoBackupEnabled: true, automaticUploadEnabled: true });
    expect(closed()).toBe(1);
  });

  it("keeps the pending name after another settings section is visited or identity is reset", async () => {
    const { state } = await dialog();
    await state.loadDevice();
    state.deviceName.value = "New PC";
    state.activeSection.value = "software";
    state.identityReset(await deviceRepository.reset());
    expect(state.deviceName.value).toBe("New PC");
    state.requestClose();
    expect(state.closeConfirmOpen.value).toBe(true);
  });
});
