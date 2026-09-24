import { t } from "./i18n";
import type { SourceKind } from "../domain";
export { sourceKey, alreadyManaged } from "./scanSources";

export type SteamUser = { accountId: string; displayName: string; steamId64?: string | null };
export type SteamSource = { path: string; kind: SourceKind; user?: SteamUser | null };
export type SteamGame = { appId: string; name: string; installPath: string; hasRules: boolean; sources: SteamSource[] };
export type SteamScanResult = { games: SteamGame[]; libraries: string[]; warnings: string[]; databaseUpdatedAt: number; scannedAt: number; steamPath: string };
export function steamUserLabel(user: SteamUser): string {
  return user.displayName === `Steam 用户 ${user.accountId}`
    ? t("Steam 用户 {id}", { id: user.accountId }) : user.displayName;
}
export function steamSourceGroups(game: SteamGame) {
  const groups = new Map<string, { id: string; label: string; archiveName: string; user?: SteamUser | null; sources: SteamSource[] }>();
  for (const source of game.sources) {
    const id = source.user?.accountId || "local";
    let group = groups.get(id);
    if (!group) {
      const label = source.user ? steamUserLabel(source.user) : t("本机存档");
      const sameName = source.user && game.sources.some(s => s.user && s.user.accountId !== id && s.user.displayName === source.user?.displayName);
      const name = source.user?.displayName || label;
      const suffix = sameName ? `${name} (${id})` : name;
      group = { id, label, user: source.user, archiveName: source.user ? `${game.name} · ${suffix}` : game.name, sources: [] };
      groups.set(id, group);
    }
    group.sources.push(source);
  }
  if (groups.size > 1 && groups.has("local")) groups.get("local")!.archiveName = t("{name} · 本机存档", { name: game.name });
  return [...groups.values()];
}
