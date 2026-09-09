export function initialExpandedSourceIds(): Set<string> {
  return new Set();
}

export function toggleExpandedSource(expandedSourceIds: ReadonlySet<string>, sourceId: string): Set<string> {
  const next = new Set(expandedSourceIds);
  if (next.has(sourceId)) next.delete(sourceId);
  else next.add(sourceId);
  return next;
}
