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
});
