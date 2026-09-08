import { describe, expect, it } from "vitest";
import { shortcutFromKeyboardEvent, shortcutMatches } from "./settings";

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
