import type { ArchiveRecord, SourceKind } from "../domain";
import { sourceKey } from "./scanSources";

export function createManagedSourceIndex(archives: ArchiveRecord[]) {
  const exact = new Set<string>(), parents = new Set<string>();
  for (const archive of archives) for (const source of archive.sources) {
    const key = sourceKey(source);
    exact.add(key);
    if (source.kind !== "file") parents.add(key);
  }
  return {
    has(source: { path: string; kind: SourceKind }): boolean {
      const key = sourceKey(source);
      if (exact.has(key)) return true;
      let slash = key.lastIndexOf("/");
      while (slash >= 0) {
        if (parents.has(key.slice(0, slash))) return true;
        slash = key.lastIndexOf("/", slash - 1);
      }
      return false;
    },
  };
}
