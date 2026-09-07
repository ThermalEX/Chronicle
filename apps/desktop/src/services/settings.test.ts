import { describe, expect, it } from "vitest";
import { shortcutMatches } from "./settings";

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
