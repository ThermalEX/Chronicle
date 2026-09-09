import { describe, expect, it } from "vitest";
import { cloudLibraryIndicator, shortcutFromKeyboardEvent, shortcutMatches, type CloudSettings } from "./settings";

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
    activeSourceId: source.id, maxConcurrentMetadataReads: 2, maxConcurrentTransfers: 2, requestDelayMs: 150, retryLimit: 5,
  };

  it("reports a configured cloud library by its source name", () => {
    expect(cloudLibraryIndicator({ ...base, enabled: true, sources: [source] })).toEqual({ available: true, label: "云端资料库：GitHub 资料库" });
  });

  it("reports disabled and multiple cloud libraries without pretending they are available", () => {
    expect(cloudLibraryIndicator({ ...base, enabled: false, sources: [source] })).toEqual({ available: false, label: "云端资料库未启用" });
    expect(cloudLibraryIndicator({ ...base, enabled: true, sources: [source, { ...source, id: "webdav-1", name: "WebDAV" }] })).toEqual({ available: true, label: "云端资料库：2 个同步源" });
  });
});
