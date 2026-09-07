import { describe, expect, it } from "vitest";
import { compareArchiveNames } from "./archiveSorting";

describe("compareArchiveNames", () => {
  const names = ["配置", "Beta", "存档", "Alpha", "游戏"];

  it("sorts Latin names A-Z before Chinese names in pinyin order", () => {
    expect([...names].sort((left, right) => compareArchiveNames(left, right))).toEqual([
      "Alpha", "Beta", "存档", "配置", "游戏",
    ]);
  });

  it("sorts both groups descending while keeping Latin names first", () => {
    expect([...names].sort((left, right) => compareArchiveNames(left, right, true))).toEqual([
      "Beta", "Alpha", "游戏", "配置", "存档",
    ]);
  });
});
