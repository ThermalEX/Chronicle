import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
import CloudCenterDialog from "./CloudCenterDialog.vue";
import { cloudRepository, type CloudPreview } from "../services/cloud";
import { cloudSettings } from "../services/settings";
import { newCloudSource } from "../services/cloudSourceControls";

const originalSources = cloudSettings.sources;

async function dialog(configure?: (state: any) => void) {
  let state: any;
  let closed = 0;
  const component = { ...CloudCenterDialog, setup(props: unknown, context: unknown) {
    state = (CloudCenterDialog as any).setup(props, context);
    configure?.(state);
    return state;
  } };
  const html = await renderToString(createSSRApp(component, { onClose: () => closed++ }));
  return { state, html, closed: () => closed };
}

function deferredPreview() {
  let resolve!: (preview: CloudPreview) => void;
  const promise = new Promise<CloudPreview>((done) => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => {
  vi.useFakeTimers();
  cloudSettings.sources = [newCloudSource("legacy_webdav", "first", 1), newCloudSource("legacy_webdav", "second", 2)];
  vi.stubGlobal("window", { setTimeout, clearTimeout });
  vi.spyOn(cloudRepository, "cancelPreview").mockResolvedValue(undefined);
});

afterEach(() => {
  cloudSettings.sources = originalSources;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.clearAllTimers();
  vi.useRealTimers();
});

describe("cloud preview dismissal", () => {
  it("cannot replace a write with a preview and enable closing", async () => {
    const preview = vi.spyOn(cloudRepository, "preview");
    const { state, closed } = await dialog((s) => { s.busy.value = "upload:archive"; });
    await state.loadPreview();
    state.requestClose();
    expect(closed()).toBe(0);
    expect(state.busy.value).toBe("upload:archive");
    expect(preview).not.toHaveBeenCalled();
  });
  it("refreshes the cloud list after an upload has finished", async () => {
    vi.spyOn(cloudRepository, "uploadApplicationSettings").mockResolvedValue(undefined);
    vi.spyOn(cloudRepository, "preview").mockResolvedValue({ sourceName: "first", items: [] });
    const { state } = await dialog();
    await state.uploadApplicationSettings();
    expect(state.preview.value).toEqual({ sourceName: "first", items: [] });
    expect(state.busy.value).toBe("");
  });
  it("closes immediately while a preview is stalled and ignores its late result", async () => {
    const pending = deferredPreview();
    vi.spyOn(cloudRepository, "preview").mockReturnValue(pending.promise);
    const { state, closed } = await dialog();
    const loading = state.loadPreview();
    state.requestClose();
    expect(closed()).toBe(1);
    pending.resolve({ sourceName: "first", items: [] });
    await loading;
    expect(state.preview.value).toBeUndefined();
  });

  it("cancels the desktop read when a stalled preview is dismissed", async () => {
    const pending = deferredPreview();
    const preview = vi.spyOn(cloudRepository, "preview").mockReturnValue(pending.promise);
    const cancel = vi.mocked(cloudRepository.cancelPreview);
    const { state } = await dialog();
    const loading = state.loadPreview();
    state.requestClose();
    await loading;
    const requestId = preview.mock.calls[0]?.[1];
    expect(typeof requestId).toBe("string");
    expect(cancel).toHaveBeenCalledWith(requestId);
    pending.resolve({ sourceName: "late", items: [] });
  });

  it("keeps Cancel enabled while only reading the preview", async () => {
    const { html } = await dialog((state) => { state.busy.value = "preview"; });
    expect(html.match(/<button class="cancel"[^>]*>/)?.[0]).not.toContain("disabled");
  });

  it("allows backdrop dismissal while only reading the preview", async () => {
    const { state, closed } = await dialog((state) => { state.busy.value = "preview"; });
    const target = {};
    const event = { target, currentTarget: target, pointerId: 1 } as unknown as PointerEvent;
    state.backdrop.pointerDown(event);
    state.backdrop.pointerUp(event);
    expect(closed()).toBe(1);
  });

  it("still asks about unsaved configuration during preview", async () => {
    const { state, closed } = await dialog((state) => { state.busy.value = "preview"; state.draft.sources[0].name = "Renamed"; });
    state.requestClose();
    expect(state.closeConfirmationOpen.value).toBe(true);
    expect(closed()).toBe(0);
  });

  it("does not close during a remote write", async () => {
    const { state, closed } = await dialog((state) => { state.busy.value = "upload:archive"; });
    state.requestClose();
    expect(closed()).toBe(0);
  });

  it("releases a stalled preview after timeout and ignores its eventual response", async () => {
    const pending = deferredPreview();
    vi.spyOn(cloudRepository, "preview").mockReturnValue(pending.promise);
    const { state } = await dialog();
    const loading = state.loadPreview();
    await vi.advanceTimersByTimeAsync(45_000);
    expect(state.busy.value).toBe("");
    expect(state.toast.value.message).toContain("超时");
    pending.resolve({ sourceName: "late", items: [] });
    await loading;
    expect(state.preview.value).toBeUndefined();
  });

  it("does not let an old source overwrite a newer source or clear its loading state", async () => {
    const first = deferredPreview();
    const second = deferredPreview();
    vi.spyOn(cloudRepository, "preview").mockImplementation((sourceId) => sourceId === "first" ? first.promise : second.promise);
    const { state } = await dialog();
    const loadingFirst = state.loadPreview();
    state.selectRepositorySource("second");
    first.resolve({ sourceName: "first", items: [] });
    await loadingFirst;
    expect(state.preview.value).toBeUndefined();
    expect(state.busy.value).toBe("preview");
    second.resolve({ sourceName: "second", items: [] });
    await vi.advanceTimersByTimeAsync(0);
    expect(state.preview.value.sourceName).toBe("second");
    expect(state.busy.value).toBe("");
  });

  it("a queued timeout from the previous preview cannot cancel the new source", async () => {
    const first = deferredPreview();
    const second = deferredPreview();
    const preview = vi.spyOn(cloudRepository, "preview").mockImplementation((sourceId) => sourceId === "first" ? first.promise : second.promise);
    const timers = vi.spyOn(window, "setTimeout");
    const { state } = await dialog();
    const loadingFirst = state.loadPreview();
    const oldTimeout = timers.mock.calls.find((call) => call[1] === 45_000)![0] as () => void;
    state.selectRepositorySource("second");
    oldTimeout();
    const secondRequestId = preview.mock.calls[1]?.[1];
    expect(cloudRepository.cancelPreview).not.toHaveBeenCalledWith(secondRequestId);
    first.resolve({ sourceName: "first", items: [] });
    second.resolve({ sourceName: "second", items: [] });
    await loadingFirst;
    await vi.advanceTimersByTimeAsync(0);
    expect(state.preview.value.sourceName).toBe("second");
  });
});
