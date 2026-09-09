import { describe, expect, it } from "vitest";
import { initialExpandedSourceIds, toggleExpandedSource } from "./sourceCardState";

describe("cloud source card expansion", () => {
  it("starts with only the active source expanded", () => {
    expect([...initialExpandedSourceIds("r2")]).toEqual(["r2"]);
    expect([...initialExpandedSourceIds(null)]).toEqual([]);
  });

  it("toggles one source without changing the others", () => {
    const expanded = new Set(["webdav"]);
    expect([...toggleExpandedSource(expanded, "github")]).toEqual(["webdav", "github"]);
    expect([...toggleExpandedSource(expanded, "webdav")]).toEqual([]);
  });
});
