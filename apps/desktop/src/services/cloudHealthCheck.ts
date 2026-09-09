export type CloudHealthCheckSource = { id: string; name: string };
export type CloudHealthCheckItem = CloudHealthCheckSource & {
  status: "checking" | "passed" | "failed";
  reason?: string;
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function runCloudHealthCheck<T extends CloudHealthCheckSource>(
  sources: readonly T[],
  checkSource: (source: T) => Promise<void>,
  onUpdate: (items: CloudHealthCheckItem[]) => void,
): Promise<CloudHealthCheckItem[]> {
  let items: CloudHealthCheckItem[] = sources.map(({ id, name }) => ({ id, name, status: "checking" }));
  onUpdate(items);
  await Promise.all(sources.map(async (source) => {
    try {
      await checkSource(source);
      items = items.map((item) => item.id === source.id ? { ...item, status: "passed" } : item);
    } catch (error) {
      items = items.map((item) => item.id === source.id ? { ...item, status: "failed", reason: errorMessage(error) } : item);
    }
    onUpdate(items);
  }));
  return items;
}
