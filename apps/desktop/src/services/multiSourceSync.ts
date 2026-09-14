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

export type SourceItemSyncOutcome<TItem, T> = SourceSyncOutcome<T> & { item: TItem };

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

/** Runs sources concurrently while serializing each source's own work. */
export async function runItemsBySource<TItem, T>(
  sources: CloudSource[],
  items: TItem[],
  action: (source: CloudSource, item: TItem) => Promise<T>,
  onSettled?: () => void,
): Promise<SourceItemSyncOutcome<TItem, T>[]> {
  const perSource = await Promise.all(sources.map(async (source) => {
    const outcomes: SourceItemSyncOutcome<TItem, T>[] = [];
    for (const item of items) {
      try {
        outcomes.push({ source, item, status: "fulfilled", value: await action(source, item) });
      } catch (reason) {
        outcomes.push({ source, item, status: "rejected", reason });
      } finally {
        onSettled?.();
      }
    }
    return outcomes;
  }));
  return perSource.flat();
}
