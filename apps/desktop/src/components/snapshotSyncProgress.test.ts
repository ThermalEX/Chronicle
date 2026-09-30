import { afterEach, describe, expect, it } from "vitest";
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
import SnapshotSyncDialog from "./SnapshotSyncDialog.vue";
import { setLocale } from "../services/i18n";

async function render(configure: (state: any) => void): Promise<string> {
  const dialog = { ...SnapshotSyncDialog, setup(props: unknown, context: unknown) {
    const state = (SnapshotSyncDialog as any).setup(props, context);
    configure(state);
    return state;
  } };
  return renderToString(createSSRApp(dialog, { sourceIds: ["a", "b", "c"], archives: [] }));
}

afterEach(() => setLocale("zh-CN"));

describe("sync progress feedback", () => {
  it("shows an indeterminate progress bar and spinner while waiting for the first response", async () => {
    const html = await render((state) => { state.busy.value = true; });
    expect(html).toContain('role="progressbar"');
    expect(html).toContain('sync-spinner');
    expect(html).toContain("indeterminate");
    expect(html).not.toContain("aria-valuenow");
    expect(html).toContain("0 / 3");
  });

  it("uses actual checked-source progress instead of an invented percentage", async () => {
    const html = await render((state) => { state.busy.value = true; state.sourcesChecked.value = 1; });
    expect(html).toContain('aria-valuenow="33"');
    expect(html).toContain("1 / 3");
    expect(html).not.toContain('indeterminate');
  });

  it("counts selected operations, not ancillary results, during transfers", async () => {
    const html = await render((state) => {
      state.busy.value = true;
      state.phase.value = "apply";
      state.plan.value = { id: "plan", sources: [] };
      state.selected.value = ["upload", "download"];
      state.results.value = [{ operationId: "upload", sourceId: "a", status: "success", error: null }, { operationId: "catalog", sourceId: "a", status: "success", error: null }];
    });
    expect(html).toContain('aria-valuenow="50"');
    expect(html).toContain("1 / 2");
  });

  it("does not reuse the old transfer count while enabling a protocol", async () => {
    setLocale("en");
    const html = await render((state) => {
      state.busy.value = true;
      state.phase.value = "enable";
      state.plan.value = { id: "old", sources: [] };
      state.selected.value = ["upload"];
      state.results.value = [{ operationId: "upload", sourceId: "a", status: "success", error: null }];
    });
    expect(html).toContain("Enabling sync protocol");
    expect(html).not.toContain("aria-valuenow");
  });

  it("stops the running feedback after a failure", async () => {
    const html = await render((state) => { state.busy.value = false; state.error.value = "HTTP 403"; });
    expect(html).toContain("HTTP 403");
    expect(html).not.toContain('role="progressbar"');
    expect(html).not.toContain('sync-spinner');
  });
});
