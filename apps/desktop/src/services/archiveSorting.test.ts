import { describe, expect, it } from "vitest";
import { compareArchiveNames } from "./archiveSorting";

describe("compareArchiveNames", () => {
  const names = ["配置", "Beta", "20-save", "存档", "Alpha", "3-save", "游戏"];

  it("sorts numeric names before Latin and Chinese names", () => {
    expect([...names].sort((left, right) => compareArchiveNames(left, right))).toEqual([
      "3-save", "20-save", "Alpha", "Beta", "存档", "配置", "游戏",
    ]);
  });

  it("keeps numeric names first while reversing order within every group", () => {
    expect([...names].sort((left, right) => compareArchiveNames(left, right, true))).toEqual([
      "20-save", "3-save", "Beta", "Alpha", "游戏", "配置", "存档",
    ]);
  });
});
