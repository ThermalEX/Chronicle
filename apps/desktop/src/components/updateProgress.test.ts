import { afterEach, describe, expect, it, vi } from "vitest";
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import UpdateDialog from "./UpdateDialog.vue";

const update = { title: "New", version: "2", notes: "", releaseUrl: "https://example.com", downloadUrl: "https://example.com", installer: { name: "setup.exe", browserDownloadUrl: "https://example.com", size: 100 }, checksumUrl: "sums" };
async function dialog(configure?: (state: any) => void) {
  let state: any;
  const component = { ...UpdateDialog, setup(props: unknown, context: unknown) { state = (UpdateDialog as any).setup(props, context); configure?.(state); return state; } };
  const html = await renderToString(createSSRApp(component, { update }));
  return { state, html };
}
afterEach(() => vi.resetAllMocks());
describe("update progress", () => {
  it("renders real percentage for verification and a spinner", async () => {
    const { html } = await dialog((s) => { s.installing.value = true; s.progress.value = { phase: "verify", bytes: 25, totalBytes: 100 }; });
    expect(html).toContain('role="progressbar"');
    expect(html).toContain('aria-valuenow="25"');
    expect(html).toContain("SHA-256");
    expect(html).toContain("update-spinner");
  });
  it("leaves progress indeterminate when total is unknown", async () => {
    const { html } = await dialog((s) => { s.installing.value = true; s.progress.value = { phase: "download", bytes: 25, totalBytes: null }; });
    expect(html).toContain('role="progressbar"');
    expect(html).not.toContain("aria-valuenow");
  });
  it("subscribes before starting, ignores stale events and permits retry after errors", async () => {
    let callback: any;
    const stop = vi.fn();
    vi.mocked(listen).mockImplementation(async (_event, cb) => { callback = cb; return stop; });
    const { state } = await dialog();
    vi.mocked(invoke).mockImplementation(async (_command, args: any) => {
      callback({ payload: { requestId: "old", phase: "verify", bytes: 99, totalBytes: 100 } });
      expect(state.progress.value.phase).toBe("download");
      callback({ payload: { requestId: args.requestId, phase: "download", bytes: 50, totalBytes: 100 } });
      expect(state.progress.value.bytes).toBe(50);
      throw new Error("failed");
    });
    await state.downloadAndInstall();
    expect(state.installing.value).toBe(false);
    expect(state.message.value).toBe("failed");
    expect(stop).toHaveBeenCalledOnce();
  });
});
