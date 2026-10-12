import { describe, expect, it } from "vitest";
import { commonMessages } from "../locales/common";
import { settingsMessages } from "../locales/settings";
import { dialogMessages } from "../locales/dialogs";
import { workspaceMessages } from "../locales/workspace";

describe("English language catalogs", () => {
  it("preserves every named placeholder without leaving empty translations", () => {
    const placeholders = (text: string) => [...new Set(text.match(/\{\w+\}/g) ?? [])].sort();
    for (const [source, translated] of Object.entries({ ...commonMessages, ...settingsMessages, ...dialogMessages, ...workspaceMessages })) {
      expect(translated.trim(), source).not.toBe("");
      expect(placeholders(translated), source).toEqual(placeholders(source));
    }
  });
  it("defines each source text in only one catalog", () => {
    const keys = [commonMessages, settingsMessages, dialogMessages, workspaceMessages].flatMap(Object.keys);
    expect(keys.filter((key, index) => keys.indexOf(key) !== index)).toEqual([]);
  });
});
