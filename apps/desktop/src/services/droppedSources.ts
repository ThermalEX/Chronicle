import type { ArchiveSource } from "../domain";

export interface DroppedPathDto {
  path: string;
  name: string;
  kind: "file" | "folder" | null;
  exists: boolean;
  error?: string | null;
}

export interface SkippedDroppedPath { path: string; reason: string }

function key(path: string): string {
  return path.replace(/\//g, "\\").replace(/\\+$/, "").toLocaleLowerCase();
}

export function normalizeDroppedSources(items: DroppedPathDto[], existing: ArchiveSource[]): {
  accepted: ArchiveSource[];
  skipped: SkippedDroppedPath[];
} {
  const seen = new Set(existing.map((source) => key(source.path)));
  const accepted: ArchiveSource[] = [];
  const skipped: SkippedDroppedPath[] = [];
  for (const item of items) {
    if (!item.exists || (item.kind !== "file" && item.kind !== "folder")) {
      skipped.push({ path: item.path, reason: item.error || "Invalid path" });
      continue;
    }
    const normalized = key(item.path);
    if (seen.has(normalized)) {
      skipped.push({ path: item.path, reason: "Duplicate path" });
      continue;
    }
    seen.add(normalized);
    accepted.push({ id: crypto.randomUUID(), name: item.name, path: item.path, kind: item.kind });
  }
  return { accepted, skipped };
}

export function canAcceptExternalDrop(state: { loading: boolean; dialogOpen: boolean; tutorialActive: boolean }): boolean {
  return !state.loading && !state.dialogOpen && !state.tutorialActive;
}
