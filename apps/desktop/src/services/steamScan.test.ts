import { describe, expect, it } from "vitest";
import type { ArchiveRecord } from "../domain";
import { alreadyManaged, steamSourceGroups, type SteamGame } from "./steamScan";
import { setLocale } from "./i18n";

describe("Steam scan duplicate sources", () => {
  const archives = [{ sources: [{ kind: "folder", path: "D:\\Saves\\Game" }] }] as ArchiveRecord[];
  it("recognizes Windows case, separators and children of managed folders", () => {
    expect(alreadyManaged({ kind: "folder", path: "d:/saves/game/" }, archives)).toBe(true);
    expect(alreadyManaged({ kind: "file", path: "D:/Saves/Game/slot.sav" }, archives)).toBe(true);
    expect(alreadyManaged({ kind: "folder", path: "D:/Saves/Game2" }, archives)).toBe(false);
    expect(alreadyManaged({ kind: "folder", path: "D:/Saves" }, archives)).toBe(false);
  });
});

describe("Steam account source grouping", () => {
  it("localizes generated account labels while preserving nicknames and game names", () => {
    const game: SteamGame = { appId: "42", name: "中文游戏", installPath: "D:/Steam", hasRules: true, sources: [
      { kind: "folder", path: "D:/1", user: { accountId: "123", displayName: "Steam 用户 123" } },
      { kind: "folder", path: "D:/2", user: { accountId: "456", displayName: "Steam 用户 789" } },
      { kind: "folder", path: "C:/存档" },
    ] };
    try {
      setLocale("en");
      expect(steamSourceGroups(game).map(group => group.label)).toEqual(["Steam user 123", "Steam 用户 789", "Local saves"]);
      expect(game.sources[0]?.user?.displayName).toBe("Steam 用户 123");
      expect(steamSourceGroups(game)[1]?.archiveName).toBe("中文游戏 · Steam 用户 789");
      expect(steamSourceGroups(game)[2]?.sources[0]?.path).toBe("C:/存档");
    } finally { setLocale("zh-CN"); }
  });
  it("keeps each user and unattributed local saves separate, with distinct names for duplicate nicknames", () => {
    const game: SteamGame = { appId: "42", name: "Example", installPath: "D:/Steam", hasRules: true, sources: [
      {kind:"folder",path:"D:/Steam/userdata/123/42/remote",user:{accountId:"123",displayName:"Player"}},
      {kind:"file",path:"D:/Steam/userdata/123/42/slot",user:{accountId:"123",displayName:"Player"}},
      {kind:"folder",path:"D:/Steam/userdata/456/42/remote",user:{accountId:"456",displayName:"Player"}},
      {kind:"folder",path:"C:/Saves"},
    ] };
    const groups = steamSourceGroups(game);
    expect(groups.map(g => g.archiveName)).toEqual(["Example · Player (123)","Example · Player (456)","Example · 本机存档"]);
    expect(groups.map(g => g.sources.length)).toEqual([2,1,1]);
    expect(steamSourceGroups({...game,sources:[game.sources[0]!]})[0]?.archiveName).toBe("Example · Player");
    expect(steamSourceGroups({...game,sources:[game.sources[3]!]})[0]?.archiveName).toBe("Example");
  });
});
