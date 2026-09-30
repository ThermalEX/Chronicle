import { describe, expect, it } from "vitest";
import { defaultSelection, executable, quickSyncPlan, type SyncOperation, type SourcePlan } from "./snapshotSync";
const source = (operations: SyncOperation[], extra: Partial<SourcePlan> = {}): SourcePlan => ({ sourceId: "dav", sourceName: "WebDAV", upgradeRequired: false, error: null, operations, archiveNames: {}, devices: [], ...extra });
const op = (action: SyncOperation["action"], id = action): SyncOperation => ({ id, action, selected: action === "upload" || action === "download", snapshot: { id, entry_id: "game", title: "first", created_at_ms: 1, size_bytes: 1, device_id: "PC", device_name: "Desktop" }, reason: null, deletion: null });
describe("sync selection", () => {
  it("runs normal transfers directly unless detailed preview is enabled", () => {
    const plan = { id: "plan", sources: [source([op("upload"), op("download"), op("unchanged")])] };
    expect(quickSyncPlan(plan, false)).toEqual({ requiresReview: false, operationIds: ["upload", "download"] });
    expect(quickSyncPlan(plan, true).requiresReview).toBe(true);
  });
  it.each(["recycleLocal", "recycleRemote", "conflict"] as const)("requires review for %s instead of silently applying it", (action) => {
    expect(quickSyncPlan({ id: "plan", sources: [source([op("upload"), op(action)])] }, false).requiresReview).toBe(true);
  });
  it("never selects transfers from failed or unupgraded sources", () => {
    const plan = { id: "plan", sources: [source([op("upload")], { error: "403" }), source([op("download")], { upgradeRequired: true }), source([op("uploadRevision")])] };
    expect(quickSyncPlan(plan, false)).toEqual({ requiresReview: true, operationIds: ["uploadRevision"] });
  });
  it("selects only safe transfers by default, leaving deletes and conflicts unselected", () => {
    expect(defaultSelection([op("upload"), op("download"), op("recycleLocal"), op("recycleRemote"), op("conflict"), op("unchanged")])).toEqual(["upload", "download"]);
  });
  it("does not allow conflicts or no-op entries to execute", () => {
    expect(executable(op("conflict"))).toBe(false);
    expect(executable(op("unchanged"))).toBe(false);
    expect(executable(op("recycleRemote"))).toBe(true);
  });
});
