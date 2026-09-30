import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { setLocale, t } from "../services/i18n";

describe("snapshot and device controls", () => {
  it("uses the same themed button primitives for every new component", () => {
    for (const file of ["DeviceSettings.vue", "SnapshotRecovery.vue", "SnapshotSyncDialog.vue"]) {
      const source = readFileSync(new URL(file, import.meta.url), "utf8");
      for (const button of source.matchAll(/<button\b[^>]*>/g)) expect(button[0], file).toContain("snapshot-control");
    }
  });
  it("uses theme fields rather than browser-default device name input", () => {
    expect(readFileSync(new URL("DeviceSettings.vue", import.meta.url), "utf8")).toContain('class="snapshot-field"');
  });
  it("translates the literal labels in the new snapshot and device pages", () => {
    setLocale("en");
    try {
      for (const file of ["DeviceSettings.vue", "SnapshotRecovery.vue", "SnapshotSyncDialog.vue"]) {
        const source = readFileSync(new URL(file, import.meta.url), "utf8");
        for (const match of source.matchAll(/\bt\('([^']+)'\)/g)) expect(t(match[1]), match[1]).not.toBe(match[1]);
      }
    } finally { setLocale("zh-CN"); }
  });
  it("translates application-owned sync failures without changing third-party errors", () => {
    setLocale("en");
    try {
      const source = readFileSync(new URL("../../src-tauri/src/snapshot_sync.rs", import.meta.url), "utf8").split("#[cfg(test)]")[0];
      for (const match of source.matchAll(/(?:Err\(|ok_or\(|map_err\(\|_\| )"([^"\n]+)"/g)) expect(t(match[1]), match[1]).not.toBe(match[1]);
      expect(t("HTTP 403: example provider error")).toBe("HTTP 403: example provider error");
    } finally { setLocale("zh-CN"); }
  });
});
