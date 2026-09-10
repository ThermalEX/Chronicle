import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const componentFiles = [
  "../App.vue",
  "ArchiveMetadata.vue",
  "CloudCenterDialog.vue",
  "AppToast.vue",
  "ConfirmDialog.vue",
  "CreateArchiveDialog.vue",
  "CreateCategoryDialog.vue",
  "SettingsDialog.vue",
  "ShortcutRecorder.vue",
  "ThemedSelect.vue",
];

describe("icon button tooltips", () => {
  it.each(componentFiles)("pairs every accessible button label with a hover title in %s", (fileName) => {
    const source = readFileSync(new URL(`./${fileName}`, import.meta.url), "utf8");
    const buttonsWithLabels = source.match(/<button\b(?=[^>]*aria-label)[^>]*>/g) ?? [];
    const missingTitles = buttonsWithLabels.filter((button) => !/(?:\s|:)title=/.test(button));

    expect(missingTitles).toEqual([]);
  });

  it("opens the archive editor with source selection highlighted from a location warning", () => {
    const app = readFileSync(new URL("../App.vue", import.meta.url), "utf8");
    const dialog = readFileSync(new URL("./CreateArchiveDialog.vue", import.meta.url), "utf8");

    expect(app).toContain('openArchiveEditor(item, true)');
    expect(app).toContain(':highlight-sources="highlightSources"');
    expect(dialog).toContain('highlightSources?: boolean');
    expect(dialog).toContain("source-location-required");
  });
});
