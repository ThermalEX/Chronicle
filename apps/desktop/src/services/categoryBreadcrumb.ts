import type { CategoryRecord } from "../domain";

const libraryRootLabel = "资料库";

export function categoryBreadcrumb(categories: CategoryRecord[], categoryId: string): string {
  if (categoryId === "all") return libraryRootLabel;

  const byId = new Map(categories.map((category) => [category.id, category]));
  const names: string[] = [];
  const visited = new Set<string>();
  let current = byId.get(categoryId);

  while (current && !visited.has(current.id)) {
    visited.add(current.id);
    names.unshift(current.name);
    current = current.parentId ? byId.get(current.parentId) : undefined;
  }

  return [libraryRootLabel, ...names].join(" / ");
}
