import { describe, expect, it } from "vitest";

import type { SnapshotRecord } from "../domain";
import { filterTimeline } from "./snapshotTimeline";

const snapshots: SnapshotRecord[] = [
  { id: "older", archiveId: "entry", title: "创建前", createdAt: 10, totalBytes: 10, contentHash: "a", files: [], changes: { added: 0, modified: 0, deleted: 0 }, safety: false },
  { id: "newer", archiveId: "entry", title: "Boss 前", note: "进入第二阶段前", createdAt: 20, totalBytes: 20, contentHash: "b", files: [], changes: { added: 0, modified: 0, deleted: 0 }, safety: false },
];

describe("snapshot timeline", () => {
  it("filters descriptions and sorts newest snapshots first by default", () => {
    expect(filterTimeline(snapshots, "boss", "newest").map((snapshot) => snapshot.id)).toEqual(["newer"]);
    expect(filterTimeline(snapshots, "第二阶段", "newest").map((snapshot) => snapshot.id)).toEqual(["newer"]);
    expect(filterTimeline(snapshots, "", "oldest").map((snapshot) => snapshot.id)).toEqual(["older", "newer"]);
  });
});
