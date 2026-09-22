import { afterEach, describe, expect, it, vi } from "vitest";
import { createSSRApp, effectScope, ssrContextKey } from "vue";
import { renderToString } from "@vue/server-renderer";
import SettingsDialog from "./SettingsDialog.vue";
import CloudHealthDialog from "./CloudHealthDialog.vue";
import OpenDalSourceFields from "./OpenDalSourceFields.vue";
import { appSettings, resetAppSettings } from "../services/settings";
import { locale, setLocale } from "../services/i18n";

afterEach(() => { resetAppSettings(); setLocale("zh-CN"); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

function dialog() {
  // Exercise setup handlers without mounting the desktop UI or loading repositories.
  vi.spyOn(console, "warn").mockImplementation(() => {});
  const scope = effectScope();
  const app = createSSRApp({});
  app.provide(ssrContextKey, {});
  const state = scope.run(() => app.runWithContext(() => (SettingsDialog as any).setup({ updateChecking: false }, { emit: () => {}, expose: () => {} })));
  return { state, stop: () => scope.stop() };
}

describe("settings language selection", () => {
  it("saves only language immediately, keeps other edits pending, and does not revert language on later save", async () => {
    const stored = new Map<string, string>();
    vi.stubGlobal("localStorage", { setItem: (key: string, value: string) => stored.set(key, value) });
    const { state, stop } = dialog();
    try {
      state.draft.retentionCount = 7;
      await state.updateLanguage("en");
      expect(locale.value).toBe("en");
      expect(appSettings.retentionCount).toBeNull();
      expect(state.draft.retentionCount).toBe(7);
      expect(state.draft.language).toBe("en");
      expect(JSON.parse(stored.get("chronicle.app-settings.v2")!).language).toBe("en");
      expect(state.sections.value[0].label).toBe("General");
      expect(state.colorModeOptions.value[0].label).toBe("Light");
      await state.save();
      expect(appSettings.language).toBe("en");
      expect(appSettings.retentionCount).toBe(7);
    } finally { stop(); }
  });

  it("restores the saved language on persistence failure and shows the original error", async () => {
    vi.stubGlobal("localStorage", { setItem: () => { throw new Error("Storage is unavailable"); } });
    const { state, stop } = dialog();
    try {
      await state.updateLanguage("en");
      expect(locale.value).toBe("zh-CN");
      expect(appSettings.language).toBe("zh-CN");
      expect(state.draft.language).toBe("zh-CN");
      expect(state.languageError.value).toBe("Storage is unavailable");
      expect(state.saving.value).toBe(false);
    } finally { stop(); }
  });

  it("renders English cloud status while preserving user names and external errors", async () => {
    setLocale("en");
    const html = await renderToString(createSSRApp(CloudHealthDialog, {
      running: false,
      items: [{ id: "source", name: "我的存档", status: "failed", reason: "服务商错误 503" }],
    }));
    expect(html).toContain("Connection check");
    expect(html).toContain("Check complete");
    expect(html).toContain("我的存档");
    expect(html).toContain("服务商错误 503");
    expect(html).not.toContain("关闭云端检测");
  });

  it("renders translated OpenDAL templates and secret labels without changing field identifiers", async () => {
    setLocale("en");
    const html = await renderToString(createSSRApp(OpenDalSourceFields, {
      source: { id: "source", name: "我的云端", provider: "opendal", scheme: "s3", endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "", config: { bucket: "my-bucket" }, secretKeys: ["secret_access_key"] },
      secrets: {},
    }));
    expect(html).toContain("S3 / S3-compatible services");
    expect(html).toContain("secret_access_key secret configuration");
    expect(html).toContain("我的云端");
    expect(html).toContain("my-bucket");
    expect(html).not.toContain("存储服务模板");
  });
});
