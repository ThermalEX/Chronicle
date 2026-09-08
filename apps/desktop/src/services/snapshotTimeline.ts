import type { SnapshotRecord } from "../domain";

export type TimelineSort = "newest" | "oldest";

export function filterTimeline(snapshots: SnapshotRecord[], query: string, sort: TimelineSort): SnapshotRecord[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  return snapshots
    .filter((snapshot) => !normalizedQuery || snapshot.title.toLocaleLowerCase().includes(normalizedQuery))
    .slice()
    .sort((left, right) => sort === "newest" ? right.createdAt - left.createdAt : left.createdAt - right.createdAt);
}
