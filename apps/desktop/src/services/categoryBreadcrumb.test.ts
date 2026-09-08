import { describe, expect, it } from "vitest";
import { categoryBreadcrumb } from "./categoryBreadcrumb";

describe("categoryBreadcrumb", () => {
  it("shows the library-rooted path for a nested category", () => {
    expect(categoryBreadcrumb([
      { id: "games", name: "游戏" },
      { id: "galgame", name: "galgame", parentId: "games" },
    ], "galgame")).toEqual("资料库 / 游戏 / galgame");
  });

  it("uses the library root for the all-archives view and tolerates a missing parent", () => {
    expect(categoryBreadcrumb([], "all")).toBe("资料库");
    expect(categoryBreadcrumb([{ id: "orphan", name: "孤立分类", parentId: "missing" }], "orphan"))
      .toBe("资料库 / 孤立分类");
  });
});
