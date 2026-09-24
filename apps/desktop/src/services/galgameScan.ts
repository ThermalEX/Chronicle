import type { ArchiveRecord } from "../domain";
import type { ArchiveRepository } from "./repository";
import { alreadyManaged, sourceKey } from "./scanSources";
import { t } from "./i18n";

export type GalgameSource = { path: string; kind: "file" | "folder"; evidence: "engine" | "directory" | "name" };
export type Galgame = { name: string; installPath: string; engine: string; sources: GalgameSource[] };
export type GalgameScanResult = { rootPath: string; scannedAt: number; games: Galgame[]; warnings: string[] };
export type GalgameProgress = { taskId: string; checkedDirectories: number; foundGames: number };
export function galgameKey(game: Galgame, source: GalgameSource) { return `${game.installPath}:${sourceKey(source)}`; }
export function uniqueSources(sources: GalgameSource[]): GalgameSource[] {
  const unique = [...new Map(sources.map(source => [sourceKey(source), source])).values()];
  return unique.filter(source => !unique.some(other => other !== source && other.kind === "folder" && sourceKey(source).startsWith(`${sourceKey(other)}/`)));
}

export async function importGalgameSelection(games: Galgame[], selected: string[], initialSnapshot: boolean,
  repository: Pick<ArchiveRepository, "listArchives" | "createArchive" | "createSnapshot">,
  validate: (sources: GalgameSource[]) => Promise<string[]>) {
  const archives = await repository.listArchives();
  const failures: string[] = [];
  let added = 0;
  for (const game of games) {
    const sources = uniqueSources(game.sources.filter(source => selected.includes(galgameKey(game, source)) && !alreadyManaged(source, archives)));
    if (!sources.length) continue;
    let created: ArchiveRecord | undefined;
    try {
      const invalid = await validate(sources);
      if (invalid.length) throw new Error(t("存档路径已失效或不可读取，请刷新扫描：{paths}", { paths: invalid.join("\n") }));
      created = await repository.createArchive({ name: game.name, sources: sources.map(source => ({ id: crypto.randomUUID(), name: source.path.split(/[\\/]/).at(-1) || game.name, path: source.path, kind: source.kind })), storagePolicy: "local", syncMode: "manual", autoBackupEnabled: false, automaticUploadEnabled: false, createInitialSnapshot: false });
      archives.push(created); added++;
      if (initialSnapshot) await repository.createSnapshot(created, t("初始版本"));
    } catch (error) { failures.push(`${game.name}：${created ? t("存档已添加，初始备份失败：") : t("添加失败：")}${String(error)}`); }
  }
  return { added, failures, archives };
}

export function galgameError(error: unknown) {
  const message = String(error);
  const keys: Record<string, string> = {
    "galgame-invalid-root": "请选择可读取的本地合集文件夹，链接目录和云端占位目录不参与扫描。",
    "galgame-cache-corrupt": "上次扫描结果无法读取，请刷新扫描。",
    "galgame-scan-busy": "已有扫描正在进行，请稍候。",
    "galgame-windows-only": "此功能仅在 Windows 桌面版可用。",
  };
  return keys[message] ? t(keys[message]) : message;
}
