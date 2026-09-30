import { afterEach, describe, expect, it, vi } from "vitest";
import { animateAppearance, applyAppearance, normalizeAppearance, resolvedColorMode, watchSystemAppearance } from "./appearance";

afterEach(() => vi.unstubAllGlobals());

describe("appearance settings", () => {
  it("keeps valid saved themes and falls back safely for older settings", () => {
    expect(normalizeAppearance({ colorTheme: "violet", colorMode: "dark" })).toEqual({
      colorTheme: "violet",
      colorMode: "dark",
    });
    expect(normalizeAppearance({ colorTheme: "unknown", colorMode: "system" })).toEqual({
      colorTheme: "teal",
      colorMode: "system",
    });
  });

  it("switches only between light and dark while keeping the chosen color", () => {
    expect(normalizeAppearance({ colorTheme: "amber", colorMode: "light" }, true)).toEqual({
      colorTheme: "amber",
      colorMode: "dark",
    });
  });

  it("accepts the neutral gray theme", () => {
    expect(normalizeAppearance({ colorTheme: "gray", colorMode: "dark" })).toEqual({
      colorTheme: "gray",
      colorMode: "dark",
    });
  });

  it("uses the system for new settings without replacing an explicit saved mode", () => {
    expect(normalizeAppearance({}).colorMode).toBe("system");
    expect(normalizeAppearance({ colorMode: "light" }).colorMode).toBe("light");
    expect(normalizeAppearance({ colorMode: "dark" }).colorMode).toBe("dark");
  });

  it("switches away from the actual system appearance when manually toggled", () => {
    vi.stubGlobal("window", { matchMedia: () => ({ matches: true }) });
    expect(normalizeAppearance({ colorTheme: "rose", colorMode: "system" }, true)).toEqual({
      colorTheme: "rose", colorMode: "light",
    });
    vi.stubGlobal("window", { matchMedia: () => ({ matches: false }) });
    expect(normalizeAppearance({ colorMode: "system" }, true).colorMode).toBe("dark");
  });
});

describe("system appearance", () => {
  it("updates on system changes only while selected, and stops listening on cleanup", () => {
    const query = Object.assign(new EventTarget(), { matches: false });
    const root = { dataset: {} as Record<string, string> };
    vi.stubGlobal("window", { matchMedia: () => query });
    vi.stubGlobal("document", { documentElement: root });
    const settings = normalizeAppearance({ colorTheme: "indigo", colorMode: "system" });
    applyAppearance(settings);
    const stop = watchSystemAppearance(() => settings);

    expect(root.dataset.colorMode).toBe("light");
    query.matches = true;
    query.dispatchEvent(new Event("change"));
    expect(root.dataset.colorMode).toBe("dark");
    expect(resolvedColorMode.value).toBe("dark");
    expect(settings.colorMode).toBe("system");

    settings.colorMode = "light";
    applyAppearance(settings);
    query.dispatchEvent(new Event("change"));
    expect(root.dataset.colorMode).toBe("light");

    settings.colorMode = "system";
    stop();
    query.dispatchEvent(new Event("change"));
    expect(root.dataset.colorMode).toBe("light");
  });

  it.each(["reduced motion", "older runtime"])("applies the theme without animation for %s", async (reason) => {
    const root = { dataset: { colorTheme: "teal", colorMode: "light" } };
    vi.stubGlobal("document", {
      documentElement: root,
      ...(reason === "reduced motion" ? { startViewTransition: () => { throw new Error("Animation should be disabled"); } } : {}),
    });
    vi.stubGlobal("window", { matchMedia: () => ({ matches: reason === "reduced motion" }) });
    await animateAppearance({ colorTheme: "violet", colorMode: "dark" });
    expect(root.dataset).toEqual({ colorTheme: "violet", colorMode: "dark" });
    expect(resolvedColorMode.value).toBe("dark");
  });
});
