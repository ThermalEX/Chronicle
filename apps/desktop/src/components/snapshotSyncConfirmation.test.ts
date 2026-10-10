import { afterEach, expect, it, vi } from "vitest";
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
import SnapshotSyncDialog from "./SnapshotSyncDialog.vue";
import { setLocale } from "../services/i18n";
import { snapshotSync, type SyncOperation, type SyncPlan } from "../services/snapshotSync";

afterEach(() => { vi.restoreAllMocks(); setLocale("zh-CN"); });
const operation = (id: string, action: SyncOperation["action"]): SyncOperation => ({
  id, action, selected: false, snapshot: { id, entry_id: "game", title: `Snapshot ${id}`, created_at_ms: 1, size_bytes: 1 }, reason: null, deletion: null,
});
const plan: SyncPlan = { id: "plan", sources: [
  { sourceId: "dav", sourceName: "Cloud One", error: null, upgradeRequired: false, devices: [], archiveNames: {}, operations: [operation("upload", "upload"), operation("local", "recycleLocal"), operation("remote", "recycleRemote"), operation("conflict", "conflict"), operation("noop", "unchanged")] },
  { sourceId: "failed", sourceName: "Offline", error: "403", upgradeRequired: false, devices: [], archiveNames: {}, operations: [operation("failed-delete", "recycleRemote")] },
  { sourceId: "old", sourceName: "Needs upgrade", error: null, upgradeRequired: true, devices: [], archiveNames: {}, operations: [operation("old-delete", "recycleRemote")] },
] };
const props = { sourceIds: ["dav", "failed", "old"], archives: [{ id: "game", name: "Game A" }] };
async function dialog() {
  let state: any;
  await renderToString(createSSRApp({ ...SnapshotSyncDialog, setup(p: unknown, c: unknown) {
    state = (SnapshotSyncDialog as any).setup(p, c); return state;
  } }, props));
  vi.spyOn(snapshotSync, "preview").mockResolvedValue(plan);
  await state.preview();
  const render = () => renderToString(createSSRApp({ ...SnapshotSyncDialog, setup: () => state }, props));
  return { state, render };
}
it("defaults to recycling only available sources and asks for confirmation without applying", async () => {
  const { state, render } = await dialog();
  expect(state.selected.value).toEqual(["upload", "local", "remote"]);
  state.selected.value = []; state.selectDeletes();
  expect(state.selected.value).toEqual(["local", "remote"]);
  const applying = vi.spyOn(snapshotSync, "apply").mockResolvedValue([]);
  await state.apply();
  expect(applying).not.toHaveBeenCalled();
  expect(state.phase.value).toBe("preview");
  const html = await render();
  expect(html).toContain('role="alertdialog"');
  expect(html).toContain("Cloud One · Game A");
  expect(html).toContain("Snapshot local");
  expect(html).toContain("Snapshot remote");
  expect(html).toContain("取消选择全部回收项");
  state.dismissRecycleConfirmation();
  expect(await render()).not.toContain('role="alertdialog"');
  expect(applying).not.toHaveBeenCalled();
});
it("toggles all recycling without changing the transfer selection", async () => {
  const { state, render } = await dialog();
  state.selected.value = ["upload", "local", "remote"];
  state.selectDeletes();
  expect(state.selected.value).toEqual(["upload"]);
  expect(await render()).toContain("选择全部回收项");
  state.selectDeletes();
  expect(state.selected.value).toEqual(["upload", "local", "remote"]);
  expect(await render()).toContain("取消选择全部回收项");
});
it("executes exactly the confirmed selection once, even if the draft selection changes", async () => {
  const { state } = await dialog();
  const applying = vi.spyOn(snapshotSync, "apply").mockResolvedValue([]);
  await state.apply();
  expect(applying).not.toHaveBeenCalled();
  state.selected.value = ["noop", "conflict", "old-delete"];
  await state.confirmRecycle();
  expect(applying).toHaveBeenCalledExactlyOnceWith("plan", ["upload", "local", "remote"]);
  expect(state.appliedSelection.value).toEqual(["upload", "local", "remote"]);
  await state.confirmRecycle();
  expect(applying).toHaveBeenCalledTimes(1);
});
it("does not require recycling confirmation for transfer-only selections", async () => {
  const { state, render } = await dialog();
  state.selected.value = ["upload"];
  const applying = vi.spyOn(snapshotSync, "apply").mockResolvedValue([]);
  await state.apply();
  expect(applying).toHaveBeenCalledExactlyOnceWith("plan", ["upload"]);
  expect(await render()).not.toContain('role="alertdialog"');
});
it("does not apply a confirmation after the plan has changed, and translates it", async () => {
  const { state, render } = await dialog(); setLocale("en");
  const applying = vi.spyOn(snapshotSync, "apply").mockResolvedValue([]);
  await state.apply();
  expect(await render()).toContain("Confirm recycling");
  state.plan.value = { ...plan, id: "new-plan" };
  await state.confirmRecycle();
  expect(applying).not.toHaveBeenCalled();
  expect(state.error.value).toContain("Preview again");
});
