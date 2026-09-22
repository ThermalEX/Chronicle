import { afterEach, describe, expect, it, vi } from "vitest";
import { appSettings, initializeSettings, normalizeAppSettings, resetAppSettings, saveAppSettings } from "./settings";
import { locale } from "./i18n";

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => false }));
afterEach(() => { resetAppSettings(); vi.unstubAllGlobals(); });

describe("saved application language", () => {
  it("keeps old settings Chinese and rejects unsupported locale values", () => {
    expect(normalizeAppSettings({}).language).toBe("zh-CN");
    expect(normalizeAppSettings({ language: "en" }).language).toBe("en");
    expect(normalizeAppSettings({ language: "bad" as never }).language).toBe("zh-CN");
  });
  it("saves and restores English without changing user settings", async () => {
    const values = new Map<string, string>();
    vi.stubGlobal("localStorage", { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value) });
    await saveAppSettings({ ...appSettings, language: "en", recycleBinPath: "D:/我的存档" });
    expect(locale.value).toBe("en");
    resetAppSettings();
    await initializeSettings();
    expect(appSettings.language).toBe("en");
    expect(locale.value).toBe("en");
    expect(appSettings.recycleBinPath).toBe("D:/我的存档");
  });
  it("rolls back a failed language save", async () => {
    vi.stubGlobal("localStorage", { setItem: () => { throw new Error("disk full"); } });
    await expect(saveAppSettings({ ...appSettings, language: "en" })).rejects.toThrow("disk full");
    expect(appSettings.language).toBe("zh-CN");
    expect(locale.value).toBe("zh-CN");
  });
});
