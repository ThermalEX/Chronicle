import type { ArchiveRecord, SourceKind } from "../domain";
type Source = { path: string; kind: SourceKind };
export function sourceKey(source: Source): string {
  return `${source.kind === "registry" ? "registry" : "file"}:${source.path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase()}`;
}
export function alreadyManaged(source: Source, archives: ArchiveRecord[]): boolean {
  const key = sourceKey(source);
  return archives.some(archive => archive.sources.some(existing => {
    const existingKey = sourceKey(existing);
    return key === existingKey || (existing.kind !== "file" && key.startsWith(`${existingKey}/`));
  }));
}
