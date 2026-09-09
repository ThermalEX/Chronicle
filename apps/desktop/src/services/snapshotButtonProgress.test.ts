import { describe, expect, it } from "vitest";
import { snapshotButtonProgress } from "./snapshotButtonProgress";

describe("snapshotButtonProgress", () => {
  it("uses an indeterminate state while the file count is unknown", () => {
    expect(snapshotButtonProgress({ current: 0, total: 0, currentPath: "扫描文件" })).toEqual({ state: "indeterminate", percent: 0 });
  });

  it("maps known snapshot progress to a rounded button fill percentage", () => {
    expect(snapshotButtonProgress({ current: 3, total: 8, currentPath: "读取文件" })).toEqual({ state: "determinate", percent: 38 });
  });
});
