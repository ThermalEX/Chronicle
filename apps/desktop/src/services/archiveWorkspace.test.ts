import { describe, expect, it } from "vitest";
import { selectArchivePanelCategory, selectCategoryPanel } from "./archiveWorkspace";

describe("archive workspace panel state", () => {
  it("collapses an already-open category and reopens it on the next click", () => {
    expect(selectCategoryPanel({ categoryId: "games", collapsed: false }, "games")).toEqual({
      categoryId: "games",
      collapsed: true,
    });
    expect(selectCategoryPanel({ categoryId: "games", collapsed: true }, "games")).toEqual({
      categoryId: "games",
      collapsed: false,
    });
  });

  it("opens the archive list when selecting a different category", () => {
    expect(selectCategoryPanel({ categoryId: "games", collapsed: true }, "documents")).toEqual({
      categoryId: "documents",
      collapsed: false,
    });
  });

  it("opens an archive under its parent category", () => {
    expect(selectArchivePanelCategory("games")).toEqual({ categoryId: "games", collapsed: false });
    expect(selectArchivePanelCategory()).toEqual({ categoryId: "all", collapsed: false });
  });
});
