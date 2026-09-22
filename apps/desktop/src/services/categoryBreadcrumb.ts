import { t } from "./i18n";
import type { CategoryRecord } from "../domain";

export function categoryBreadcrumb(categories: CategoryRecord[], categoryId: string): string {
  const libraryRootLabel = t("资料库");
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
