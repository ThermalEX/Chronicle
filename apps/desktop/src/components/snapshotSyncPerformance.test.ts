import { afterEach, expect, it, vi } from "vitest";
import { createSSRApp, nextTick, watchEffect } from "vue";
import { renderToString } from "@vue/server-renderer";
import SnapshotSyncDialog from "./SnapshotSyncDialog.vue";
import type { SyncOperation, SyncPlan } from "../services/snapshotSync";

afterEach(() => vi.restoreAllMocks());
function operation(id: string, entry: string, action: SyncOperation["action"]): SyncOperation {
  return { id, action, selected: true, snapshot: { id, entry_id: entry, title: id, created_at_ms: 1, size_bytes: 1 }, reason: null, deletion: null };
}
it("does not regroup plan operations on progress updates and updates row status and progress", async () => {
  let state: any, reads = 0;
  const first = operation("one", "a", "upload"), second = operation("two", "b", "download");
  for (const op of [first, second]) {
    const snapshot = op.snapshot;
    Object.defineProperty(op, "snapshot", { get() { reads++; return snapshot; }, enumerable: true });
  }
  const plan: SyncPlan = { id: "plan", sources: [{ sourceId: "source", sourceName: "Cloud", upgradeRequired: false, error: null, operations: [first, second], archiveNames: { b: "Remote B" }, devices: [] }] };
  const wrapper = { ...SnapshotSyncDialog, setup(p: unknown, c: unknown) { state = (SnapshotSyncDialog as any).setup(p, c); state.plan.value = plan; state.selected.value = ["one", "two"]; state.appliedSelection.value = ["one", "two"]; state.toggleSource("source"); state.toggleArchive("source", "a"); state.toggleArchive("source", "b"); state.phase.value = "apply"; state.busy.value = true; return state; } };
  const app = createSSRApp(wrapper, { sourceIds: ["source"], archives: [{ id: "a", name: "Local A" }] });
  const initial = await renderToString(app); expect(initial).toContain("Local A"); expect(initial).toContain("Remote B");
  reads = 0; state.results.value = [{ operationId: "one", sourceId: "source", status: "success", error: null }];
  await nextTick(); const updated = await renderToString(createSSRApp({ ...SnapshotSyncDialog, setup: () => state }, { sourceIds: ["source"], archives: [{ id: "a", name: "Local A" }] }));
  expect(updated).toContain("已完成"); expect(state.progress.value.current).toBe(1);
  // Rendering needs title/date/size/device, but no second snapshot reads for grouping.
  expect(reads).toBeLessThanOrEqual(10);
});
for (const size of [1000, 5000]) it(`indexes ${size} operations once and counts progress independently of cached groups`, async () => {
  let state: any;
  await renderToString(createSSRApp({ ...SnapshotSyncDialog, setup(p: unknown, c: unknown) { state = (SnapshotSyncDialog as any).setup(p, c); return state; } }, { sourceIds: ["source"], archives: [] }));
  const operations = Array.from({ length: size }, (_, i) => operation(String(i), `entry-${i % 10}`, i % 2 ? "download" : "upload"));
  state.plan.value = { id: "scale", sources: [{ sourceId: "source", sourceName: "Cloud", upgradeRequired: false, error: null, operations, archiveNames: {}, devices: [] }] };
  state.selected.value = operations.map(op => op.id); state.appliedSelection.value = [...state.selected.value]; state.phase.value = "apply";
  // Subscribe as a client render does; SSR itself deliberately leaves computeds uncached.
  const stop = watchEffect(() => { state.sourceViews.value; });
  const source = state.sourceViews.value[0];
  expect(source.groups).toHaveLength(10); expect(source.counts.upload).toBe(size / 2); expect(source.counts.download).toBe(size / 2);
  state.results.value = operations.slice(0, size / 2).map(op => ({ operationId: op.id, sourceId: "source", status: "success", error: null }));
  expect(state.progress.value).toMatchObject({ current: size / 2, total: size, percent: 50 });
  expect(state.sourceViews.value[0]).toBe(source);
  state.results.value = [{ operationId: "0", sourceId: "source", status: "failed", error: "Offline" }, { operationId: "catalog", sourceId: "source", status: "success", error: null }];
  expect(state.resultsById.value.get("0").error).toBe("Offline"); expect(state.ancillaryResults.value).toHaveLength(1);
  expect(state.progress.value.current).toBe(1);
  stop();
});
it("does not rescan 5000 static selected IDs for each progress delta", async () => {
  let state: any, reads = 0;
  await renderToString(createSSRApp({ ...SnapshotSyncDialog, setup(p: unknown, c: unknown) { state = (SnapshotSyncDialog as any).setup(p, c); return state; } }, { sourceIds: ["source"], archives: [] }));
  const operations = Array.from({ length: 5000 }, (_, i) => {
    const op = operation(String(i), "entry", "upload");
    Object.defineProperty(op, "id", { enumerable: true, get() { reads++; return String(i); } });
    return op;
  });
  state.plan.value = { id: "scale", sources: [{ sourceId: "source", sourceName: "Cloud", upgradeRequired: false, error: null, operations, archiveNames: {}, devices: [] }] };
  state.appliedSelection.value = operations.map(op => op.id);
  const stop = watchEffect(() => { state.sourceProgress.value; });
  expect(state.sourceProgress.value.get("source").total).toBe(5000);
  reads = 0;
  for (let i = 0; i < 10; i++) {
    state.results.value = [{ operationId: String(i), sourceId: "source", status: "success", error: null }];
    expect(state.sourceProgress.value.get("source").current).toBe(1);
  }
  expect(reads).toBe(0);
  state.appliedSelection.value = ["0"];
  expect(state.sourceProgress.value.get("source").total).toBe(1);
  state.finishedSources.value.add("source");
  state.results.value = [{ operationId: "0", sourceId: "source", status: "success", error: null }];
  expect(state.sourceProgress.value.get("source").completed).toBe(true);
  state.plan.value.sources[0].operations = [operation("other", "entry", "upload")];
  expect(state.sourceProgress.value.get("source").total).toBe(0);
  stop();
});
