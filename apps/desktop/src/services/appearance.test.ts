import { describe, expect, it } from "vitest";
import { normalizeAppearance } from "./appearance";

describe("appearance settings", () => {
  it("keeps valid saved themes and falls back safely for older settings", () => {
    expect(normalizeAppearance({ colorTheme: "violet", colorMode: "dark" })).toEqual({
      colorTheme: "violet",
      colorMode: "dark",
    });
    expect(normalizeAppearance({ colorTheme: "unknown", colorMode: "system" })).toEqual({
      colorTheme: "teal",
      colorMode: "light",
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
});
