import { describe, expect, it } from "vitest";
import { canAcceptExternalDrop, normalizeDroppedSources, type DroppedPathDto } from "./droppedSources";

const file = (path: string, name = "save.dat"): DroppedPathDto => ({ path, name, kind: "file", exists: true });
const folder = (path: string, name = "Game"): DroppedPathDto => ({ path, name, kind: "folder", exists: true });

describe("external dropped sources", () => {
  it("accepts one file and folder as sources for one archive", () => {
    const result = normalizeDroppedSources([file("C:\\save.dat"), folder("D:\\Game")], []);
    expect(result.accepted.map((source) => source.kind)).toEqual(["file", "folder"]);
    expect(result.skipped).toEqual([]);
  });

  it("deduplicates Windows paths across case and slash variants", () => {
    const result = normalizeDroppedSources([file("C:\\Game\\save.dat"), file("c:/game/SAVE.dat")], []);
    expect(result.accepted).toHaveLength(1);
    expect(result.skipped).toHaveLength(1);
  });

  it("keeps valid items when another path is invalid", () => {
    const result = normalizeDroppedSources([
      file("C:\\save.dat"),
      { path: "C:\\missing", name: "missing", kind: null, exists: false, error: "Not found" },
    ], []);
    expect(result.accepted).toHaveLength(1);
    expect(result.skipped[0].reason).toBe("Not found");
    expect(normalizeDroppedSources([{ path: "missing", name: "missing", kind: null, exists: false }], []).accepted).toHaveLength(0);
  });

  it("blocks loading, dialogs and tutorial without touching pending sources", () => {
    expect(canAcceptExternalDrop({ loading: false, dialogOpen: false, tutorialActive: false })).toBe(true);
    expect(canAcceptExternalDrop({ loading: true, dialogOpen: false, tutorialActive: false })).toBe(false);
    expect(canAcceptExternalDrop({ loading: false, dialogOpen: true, tutorialActive: false })).toBe(false);
    expect(canAcceptExternalDrop({ loading: false, dialogOpen: false, tutorialActive: true })).toBe(false);
  });
});
