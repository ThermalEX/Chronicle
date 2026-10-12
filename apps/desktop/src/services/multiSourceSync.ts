import type { CloudSource } from "./settings";

export type SourceSyncOutcome<T> = {
  source: CloudSource;
  status: "fulfilled";
  value: T;
} | {
  source: CloudSource;
  status: "rejected";
  reason: unknown;
};

export async function runAcrossEnabledSources<T>(
  sources: CloudSource[],
  action: (source: CloudSource) => Promise<T>,
  onSettled?: () => void,
): Promise<SourceSyncOutcome<T>[]> {
  return Promise.all(sources.map(async (source): Promise<SourceSyncOutcome<T>> => {
    try {
      return { source, status: "fulfilled", value: await action(source) };
    } catch (reason) {
      return { source, status: "rejected", reason };
    } finally {
      onSettled?.();
    }
  }));
}
