import { describe, expect, it } from "vitest";
import { initialExpandedSourceIds, toggleExpandedSource } from "./sourceCardState";

describe("cloud source card expansion", () => {
  it("starts with every source collapsed, including the active one", () => {
    expect([...initialExpandedSourceIds()]).toEqual([]);
    expect([...initialExpandedSourceIds()]).toEqual([]);
  });

  it("toggles one source without changing the others", () => {
    const expanded = new Set(["webdav"]);
    expect([...toggleExpandedSource(expanded, "github")]).toEqual(["webdav", "github"]);
    expect([...toggleExpandedSource(expanded, "webdav")]).toEqual([]);
  });
});
