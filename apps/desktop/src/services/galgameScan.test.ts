import { describe, expect, it } from "vitest";
import type { ArchiveRecord, CreateArchiveInput } from "../domain";
import { galgameKey, importGalgameSelection, uniqueSources, type Galgame } from "./galgameScan";

const games: Galgame[] = ["First", "Second"].map(name => ({ name, installPath: `D:/${name}`, engine: "Kirikiri", sources: [{ path: `D:/${name}/save`, kind: "folder", evidence: "engine" }] }));
function repository() {
  const archives: ArchiveRecord[] = [];
  const inputs: CreateArchiveInput[] = [];
  return { archives, inputs, listArchives: async () => [...archives],
    createArchive: async (input: CreateArchiveInput) => { inputs.push(input); const archive: ArchiveRecord = { ...input, id: input.name, sourcePath: input.sources[0].path, category: "", tags: [], kind: "folder", createdAt: 0, updatedAt: 0, totalBytes: 0 }; archives.push(archive); return archive; },
    createSnapshot: async () => { throw new Error("snapshot failed"); },
  };
}
const selection = games.flatMap(g => g.sources.map(s => galgameKey(g, s)));
describe("Galgame scan import", () => {
  it("only creates selected games with local and manual defaults", async () => {
    const repo = repository();
    const result = await importGalgameSelection(games, selection.slice(0, 1), false, repo, async () => []);
    expect(result.added).toBe(1);
    expect(repo.inputs[0]).toMatchObject({ name: "First", storagePolicy: "local", autoBackupEnabled: false, automaticUploadEnabled: false, syncMode: "manual", createInitialSnapshot: false });
    expect(repo.inputs[0].sources[0].path).toBe("D:/First/save");
  });
  it("rechecks paths and continues after an invalid game", async () => {
    const repo = repository();
    const result = await importGalgameSelection(games, selection, false, repo, async sources => sources[0].path.includes("First") ? [sources[0].path] : []);
    expect(result.added).toBe(1);
    expect(result.failures[0]).toContain("First");
    expect(repo.inputs[0].name).toBe("Second");
  });
  it("does not recreate an archive when its initial snapshot failed", async () => {
    const repo = repository();
    const result = await importGalgameSelection(games, selection, true, repo, async () => []);
    expect(result.added).toBe(2);
    expect(result.failures).toHaveLength(2);
    const retry = await importGalgameSelection(games, selection, false, repo, async () => []);
    expect(retry.added).toBe(0);
  });
  it("deduplicates selected sources including parent folders", () => {
    expect(uniqueSources([
      { path: "D:/save/slot.sav", kind: "file", evidence: "engine" },
      { path: "d:\\SAVE", kind: "folder", evidence: "name" },
      { path: "D:/save", kind: "folder", evidence: "engine" },
    ])).toHaveLength(1);
  });
});
