import { describe, expect, it } from "vitest";
import { cloudLibraryIndicator, enabledCloudSources, normalizeAppSettings, normalizedCloud, shortcutFromKeyboardEvent, shortcutMatches, type CloudSettings } from "./settings";

function keyEvent(key: string, options: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return { key, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...options } as KeyboardEvent;
}

describe("shortcutMatches", () => {
  it("matches configured modifier combinations exactly", () => {
    expect(shortcutMatches(keyEvent("K", { ctrlKey: true }), "Ctrl+K")).toBe(true);
    expect(shortcutMatches(keyEvent("b", { ctrlKey: true, shiftKey: true }), "Ctrl+Shift+B")).toBe(true);
    expect(shortcutMatches(keyEvent("b", { ctrlKey: true }), "Ctrl+Shift+B")).toBe(false);
  });
});

describe("shortcutFromKeyboardEvent", () => {
  it("formats a captured modifier combination in stable order", () => {
    expect(shortcutFromKeyboardEvent(keyEvent("b", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+B");
    expect(shortcutFromKeyboardEvent(keyEvent(",", { ctrlKey: true }))).toBe("Ctrl+,");
  });

  it("ignores modifier-only and unmodified keys", () => {
    expect(shortcutFromKeyboardEvent(keyEvent("Control", { ctrlKey: true }))).toBeNull();
    expect(shortcutFromKeyboardEvent(keyEvent("b"))).toBeNull();
  });
});

describe("cloudLibraryIndicator", () => {
  const source = {
    id: "github-1", name: "GitHub 资料库", provider: "legacy_github" as const, endpoint: "", username: "",
    remotePath: "/Chronicle", credentialRef: "chronicle-github:github-1", repository: "owner/chronicle", branch: "main",
  };
  const base: Omit<CloudSettings, "enabled" | "sources"> = {
    maxConcurrentMetadataReads: 2, maxConcurrentTransfers: 2, requestDelayMs: 150, retryLimit: 5,
  };

  it("reports an untested configured cloud library as pending", () => {
    expect(cloudLibraryIndicator({ ...base, enabled: true, sources: [source] })).toEqual({ available: false, label: "云端资料库：未检测" });
  });

  it("reports disabled and multiple cloud libraries without pretending they are available", () => {
    expect(cloudLibraryIndicator({ ...base, enabled: false, sources: [source] })).toEqual({ available: false, label: "云端资料库未启用" });
    expect(cloudLibraryIndicator({ ...base, enabled: true, sources: [source, { ...source, id: "webdav-1", name: "WebDAV" }] })).toEqual({ available: false, label: "云端资料库：未检测" });
  });

  it("reports checked sources and identifies the first unavailable source", () => {
    const settings = { ...base, enabled: true, sources: [source, { ...source, id: "webdav-1", name: "WebDAV" }] };
    expect((cloudLibraryIndicator as any)(settings, { status: "available" })).toEqual({ available: true, label: "云端资料库：2 个同步源可用" });
    expect((cloudLibraryIndicator as any)(settings, { status: "unavailable", sourceName: "WebDAV" })).toEqual({ available: false, label: "云端资料库：WebDAV 无法使用" });
  });
});

describe("multi-source cloud settings", () => {
  it("migrates only the legacy active source into an enabled source", () => {
    const migrated = normalizedCloud({
      enabled: true,
      activeSourceId: "source-b",
      sources: [
        { id: "source-a", name: "A", provider: "legacy_webdav", endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "a" },
        { id: "source-b", name: "B", provider: "legacy_webdav", endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "b" },
      ],
    });

    expect(migrated.sources.map((item) => item.syncEnabled)).toEqual([false, true]);
    expect(enabledCloudSources(migrated).map((item) => item.id)).toEqual(["source-b"]);
  });

  it("keeps newly shaped sources paused when no legacy source is selected", () => {
    const migrated = normalizedCloud({
      enabled: true,
      sources: [{ id: "source-a", name: "A", provider: "legacy_webdav", endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "a", syncEnabled: false }],
    });

    expect(migrated.sources[0].syncEnabled).toBe(false);
    expect(enabledCloudSources(migrated)).toEqual([]);
  });
});

describe("automatic backup settings", () => {
  it("migrates an old schedule into the global backup delay only", () => {
    expect(normalizeAppSettings({ backupSchedule: "15m" })).toMatchObject({
      autoBackupDelaySeconds: 300,
    });
  });

  it("removes legacy global automation flags while keeping a five-second default", () => {
    const settings = normalizeAppSettings({ autoBackupEnabled: true, automaticUploadEnabled: true, autoBackupDelaySeconds: 0 });
    expect(settings.autoBackupDelaySeconds).toBe(5);
    expect(settings).not.toHaveProperty("autoBackupEnabled");
    expect(settings).not.toHaveProperty("automaticUploadEnabled");
  });
});

describe("update channel settings", () => {
  it("uses the stable channel when older settings do not contain a channel", () => {
    expect(normalizeAppSettings({}).updateChannel).toBe("stable");
  });

  it("keeps the explicitly selected beta channel", () => {
    expect(normalizeAppSettings({ updateChannel: "beta" } as any).updateChannel).toBe("beta");
  });
});
