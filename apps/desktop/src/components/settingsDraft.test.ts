import "fake-indexeddb/auto";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
import SettingsDialog from "./SettingsDialog.vue";
import { appSettings, resetAppSettings } from "../services/settings";
import { deviceRepository } from "../services/devices";
import { locale, setLocale } from "../services/i18n";
import { archiveRepository } from "../services/repository";

async function dialog(configure?: (state: any) => void) {
  let state: any;
  let closed = 0;
  const component = { ...SettingsDialog, setup(props: unknown, context: unknown) {
    state = (SettingsDialog as any).setup(props, context);
    configure?.(state);
    return state;
  } };
  const html = await renderToString(createSSRApp(component, { onClose: () => closed++ }));
  return { state, html, closed: () => closed };
}

beforeEach(() => {
  const values = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
    removeItem: (key: string) => { values.delete(key); },
  });
  resetAppSettings();
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
