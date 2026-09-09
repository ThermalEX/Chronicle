type SnapshotProgressLike = { current: number; total: number; currentPath?: string };

export function snapshotButtonProgress(progress?: SnapshotProgressLike): { state: "idle" | "indeterminate" | "determinate"; percent: number } {
  if (!progress) return { state: "idle", percent: 0 };
  if (!progress.total) return { state: "indeterminate", percent: 0 };
  return { state: "determinate", percent: Math.round(progress.current / progress.total * 100) };
}
