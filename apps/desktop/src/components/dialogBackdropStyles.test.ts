import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const dialogFiles = [
  "CloudCenterDialog.vue",
  "ConfirmDialog.vue",
  "CreateArchiveDialog.vue",
  "CreateCategoryDialog.vue",
  "SettingsDialog.vue",
];

describe("dialog backdrops", () => {
  it.each(dialogFiles)("uses a neutral gray overlay in %s", (fileName) => {
    const source = readFileSync(new URL(`./${fileName}`, import.meta.url), "utf8");

    expect(source).toMatch(/background:\s*#18181b99/);
    expect(source).not.toContain("#1024218a");
  });
});
