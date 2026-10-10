import { afterEach, expect, it, vi } from "vitest";
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
import SnapshotSyncDialog from "./SnapshotSyncDialog.vue";
import { setLocale } from "../services/i18n";
import { snapshotSync, type SyncOperation, type SyncPlan } from "../services/snapshotSync";

afterEach(() => { vi.restoreAllMocks(); setLocale("zh-CN"); });
const operation = (id: string, entry: string, action: SyncOperation["action"]): SyncOperation => ({
  id, action, selected: true, snapshot: { id, entry_id: entry, title: `Snapshot ${id}`, created_at_ms: 1, size_bytes: 1 }, reason: null, deletion: null,
});
const plan: SyncPlan = { id: "plan", sources: [
  { sourceId: "one", sourceName: "Cloud One", error: null, upgradeRequired: false, devices: [], archiveNames: {}, operations: [operation("upload", "a", "upload"), operation("noop", "a", "unchanged"), operation("conflict", "b", "conflict")] },
  { sourceId: "two", sourceName: "Cloud Two", error: null, upgradeRequired: false, devices: [], archiveNames: {}, operations: [operation("download", "a", "download")] },
] };
const props = { sourceIds: ["one", "two"], archives: [{ id: "a", name: "Archive A" }, { id: "b", name: "Archive B" }] };
async function dialog() {
  let state: any;
  await renderToString(createSSRApp({ ...SnapshotSyncDialog, setup(p: unknown, c: unknown) {
    state = (SnapshotSyncDialog as any).setup(p, c); state.plan.value = plan; state.selected.value = ["upload", "download"]; return state;
  } }, props));
  const render = () => renderToString(createSSRApp({ ...SnapshotSyncDialog, setup: () => state }, props));
  return { state, render };
}
it("starts both disclosure levels closed and shows change counts excluding unchanged rows", async () => {
  const { state, render } = await dialog();
  expect(await render()).not.toContain("Archive A");
  expect(state.sourceViews.value.map((s: any) => s.changeCount)).toEqual([2, 1]);
  expect(state.sourceViews.value[0].groups.map((g: any) => g.changeCount)).toEqual([1, 1]);
  state.toggleSource("one");
  expect(await render()).toContain("Archive A");
  expect(await render()).not.toContain("Snapshot upload");
  state.toggleArchive("one", "a");
  expect(await render()).toContain("Snapshot upload");
  state.toggleSource("one"); expect(await render()).not.toContain("Archive A");
  state.toggleSource("one"); expect(await render()).toContain("Snapshot upload");
});
it("tracks each source independently and only marks green after its final records succeed", async () => {
  const { state, render } = await dialog();
  let finish!: (results: unknown[]) => void;
  vi.spyOn(snapshotSync, "apply").mockReturnValue(new Promise(resolve => { finish = resolve; }) as any);
  const applying = state.apply();
  state.acceptProgress({ requestId: "plan", activeSourceId: "one", results: [{ operationId: "upload", sourceId: "one", status: "success", error: null }] });
  expect(state.sourceProgress.value.get("one")).toMatchObject({ current: 1, total: 1, completed: false });
  expect(state.sourceProgress.value.get("two")).toMatchObject({ current: 0, total: 1, completed: false });
  expect(await render()).toContain('role="progressbar"');
  state.acceptProgress({ requestId: "stale", finishedSourceId: "one" });
  expect(state.sourceProgress.value.get("one").completed).toBe(false);
  state.acceptProgress({ requestId: "plan", finishedSourceId: "one" });
  expect(state.sourceProgress.value.get("one").completed).toBe(true);
  expect(await render()).toContain("Cloud One · 同步完成");
  const results = [
    { operationId: "upload", sourceId: "one", status: "success", error: null },
    { operationId: "download", sourceId: "two", status: "failed", error: "Offline" },
  ];
  state.acceptProgress({ requestId: "plan", results, finishedSourceId: "two" });
  finish(results); await applying;
  expect(state.sourceProgress.value.get("two")).toMatchObject({ failed: true, completed: false });
  // Failures must remain visible with both disclosure levels closed.
  expect(await render()).toContain("Offline");
});
it("keeps every concurrent source active until its own final records finish", async () => {
  const { state } = await dialog();
  state.phase.value = "apply"; state.busy.value = true;
  state.requestId.value = "plan"; state.appliedSelection.value = ["upload", "download"];
  state.acceptProgress({ requestId: "plan", activeSourceId: "one" });
  state.acceptProgress({ requestId: "plan", activeSourceId: "two" });
  expect(state.sourceProgress.value.get("one").active).toBe(true);
  expect(state.sourceProgress.value.get("two").active).toBe(true);
  state.acceptProgress({ requestId: "plan", finishedSourceId: "one", results: [
    { operationId: "upload", sourceId: "one", status: "success", error: null },
  ] });
  expect(state.sourceProgress.value.get("one")).toMatchObject({ active: false, completed: true });
  expect(state.sourceProgress.value.get("two")).toMatchObject({ active: true, completed: false });
  state.acceptProgress({ requestId: "plan", finishedSourceId: "two", results: [
    { operationId: "download", sourceId: "two", status: "success", error: null },
  ] });
  expect(state.sourceProgress.value.get("one")).toMatchObject({ current: 1, completed: true });
  expect(state.sourceProgress.value.get("two")).toMatchObject({ current: 1, completed: true });
});
it("does not show success for a record failure or cancellation, and translates disclosure status", async () => {
  const { state, render } = await dialog(); setLocale("en");
  state.phase.value = "apply"; state.requestId.value = "plan";
  state.appliedSelection.value = ["upload", "download"];
  state.acceptProgress({ requestId: "plan", finishedSourceId: "one", results: [
    { operationId: "upload", sourceId: "one", status: "success", error: null },
    { operationId: "device-record", sourceId: "one", status: "failed", error: "Offline" },
  ] });
  expect(state.sourceProgress.value.get("one")).toMatchObject({ failed: true, completed: false });
  state.acceptProgress({ requestId: "plan", finishedSourceId: "two", results: [{ operationId: "download", sourceId: "two", status: "cancelled", error: null }] });
  expect(state.sourceProgress.value.get("two").completed).toBe(false);
  expect(await render()).toContain("Pending changes");
});
it("shows a selected index repair as work, while empty sources are skipped without success or a running bar", async () => {
  const { state, render } = await dialog();
  state.plan.value = { id: "plan", sources: [
    { ...plan.sources[0], operations: [], indexRepairId: "repair-index" },
    { ...plan.sources[1], operations: [] },
  ] };
  state.phase.value = "apply"; state.busy.value = true; state.appliedSelection.value = ["repair-index"];
  state.requestId.value = "plan"; state.acceptProgress({ requestId: "plan", activeSourceId: "one" });
  expect(state.sourceViews.value.map((s: any) => s.changeCount)).toEqual([1, 0]);
  expect(state.sourceProgress.value.get("one")).toMatchObject({ total: 1, completed: false });
  state.finishedSources.value.add("two");
  expect(state.sourceProgress.value.get("two").completed).toBe(false);
  expect(await render()).toContain("未选择操作，已跳过");
  expect(await render()).not.toContain("Cloud Two · 同步完成");
  expect((await render()).match(/role="progressbar"/g)).toHaveLength(2); // global + active source only
  state.phase.value = "preview"; state.toggleSource("one");
  expect(await render()).toContain("修复云端显示索引");
});
