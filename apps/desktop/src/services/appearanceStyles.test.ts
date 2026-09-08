import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const stylesheet = readFileSync(new URL("../styles.css", import.meta.url), "utf8");

function rule(selector: string): string {
  const match = stylesheet.match(new RegExp(`${selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([^}]*)\\}`));
  return match?.[1] ?? "";
}

describe("appearance stylesheet", () => {
  it.each(["indigo", "violet", "amber", "rose", "gray"])("gives %s its own structural light colors", (theme) => {
    const content = rule(`:root[data-color-theme="${theme}"]`);
    expect(content).toContain("--titlebar:");
    expect(content).toContain("--sidebar:");
    expect(content).toContain("--app-background:");
  });

  it.each(["teal", "indigo", "violet", "amber", "rose", "gray"])("keeps %s dark-mode icons legible", (theme) => {
    const selector = theme === "teal"
      ? ':root[data-color-mode="dark"]'
      : `:root[data-color-mode="dark"][data-color-theme="${theme}"]`;
    expect(rule(selector)).toContain("--icon:");
  });

  it("uses the bright titlebar foreground for idle toolbar icons", () => {
    expect(rule(".toolbar-action")).toContain("color: var(--titlebar-text)");
  });
});
