import type { ArchiveRecord, CategoryRecord } from "../domain";
import { compareArchiveNames } from "./archiveSorting";

export function buildLibraryIndex(categories: CategoryRecord[], archives: ArchiveRecord[]) {
  const categoryById = new Map(categories.map(category => [category.id, category]));
  const children = new Map<string | null, CategoryRecord[]>();
  const directArchives = new Map<string, ArchiveRecord[]>();
  const counts = new Map<string, number>();
  const descendantCache = new Map<string, Set<string>>();
  for (const category of categories) {
    const parent = category.parentId ?? null;
    const list = children.get(parent) ?? [];
    list.push(category); children.set(parent, list);
  }
  for (const list of children.values()) list.sort((left, right) => left.name.localeCompare(right.name, "zh-CN"));
  for (const archive of archives) {
    if (!archive.categoryId) continue;
    const list = directArchives.get(archive.categoryId) ?? [];
    list.push(archive); directArchives.set(archive.categoryId, list);
    const visited = new Set<string>();
    let id: string | null | undefined = archive.categoryId;
    while (id && !visited.has(id)) {
      visited.add(id); counts.set(id, (counts.get(id) ?? 0) + 1);
      id = categoryById.get(id)?.parentId;
    }
  }
  for (const list of directArchives.values()) list.sort((left, right) => compareArchiveNames(left.name, right.name));
  function descendants(id: string): Set<string> {
    const cached = descendantCache.get(id);
    if (cached) return cached;
    const result = new Set<string>(), pending = [id];
    while (pending.length) {
      const next = pending.pop()!;
      if (result.has(next)) continue;
      result.add(next);
      for (const child of children.get(next) ?? []) pending.push(child.id);
    }
    descendantCache.set(id, result);
    return result;
  }
  return { categoryById, children, directArchives, counts, descendants };
}
