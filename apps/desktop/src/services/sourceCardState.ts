export function initialExpandedSourceIds(activeSourceId: string | null): Set<string> {
  return activeSourceId ? new Set([activeSourceId]) : new Set();
}

export function toggleExpandedSource(expandedSourceIds: ReadonlySet<string>, sourceId: string): Set<string> {
  const next = new Set(expandedSourceIds);
  if (next.has(sourceId)) next.delete(sourceId);
  else next.add(sourceId);
  return next;
}
