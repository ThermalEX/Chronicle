import { afterEach, describe, expect, it, vi } from "vitest";
import { advanceTutorial, createTutorialState, loadTutorialProgress, normalizeTutorialProgress, saveTutorialProgress, shouldOfferTutorial } from "./onboarding";

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => false }));
afterEach(() => vi.unstubAllGlobals());

describe("tutorial transitions", () => {
  it("local route does not require a cloud configuration", () => {
    let state = advanceTutorial(createTutorialState(), { type: "start" });
    expect(state.step).toBe("appearance");
    state = advanceTutorial(state, { type: "next" });
    expect(state.step).toBe("route");
    state = advanceTutorial(state, { type: "choose-local" });
    expect(state.step).toBe("create-entry");
    expect(state.route).toBe("local");
  });
  it("only confirmed source and archive success advances action steps", () => {
    let state = { ...createTutorialState(), step: "sources" as const };
    expect(advanceTutorial(state, { type: "next" }).step).toBe("sources");
    expect(advanceTutorial(state, { type: "sources-ready" }).step).toBe("name");
    const submit = { ...state, step: "create-submit" as const };
    expect(advanceTutorial(submit, { type: "next" }).step).toBe("create-submit");
    expect(advanceTutorial(submit, { type: "archive-created", archiveId: "a" })).toMatchObject({ step: "snapshot", archiveId: "a" });
  });
  it("cloud cancellation can return to a purely local route", () => {
    const cloud = { ...createTutorialState(), step: "cloud-form" as const, route: "cloud" as const };
    expect(advanceTutorial(cloud, { type: "dialog-closed" }).step).toBe("cloud-entry");
    expect(advanceTutorial(cloud, { type: "choose-local" })).toMatchObject({ step: "create-entry", route: "local" });
    expect(advanceTutorial(cloud, { type: "next" }).step).toBe("cloud-form");
  });
  it("restore is an explanation transition, never a restore command", () => {
    expect(advanceTutorial({ ...createTutorialState(), step: "restore" }, { type: "next" }).step).toBe("lock");
  });
  it("existing archive replay can bypass creation and skip real backup", () => {
    const existing = advanceTutorial({ ...createTutorialState(), step: "create-entry" }, { type: "use-existing", archiveId: "old", hasSnapshot: false });
    expect(existing).toMatchObject({ step: "snapshot", archiveId: "old" });
    expect(advanceTutorial(existing, { type: "next" }).step).toBe("sync");
  });
  it("skipping always exits and stale success cannot resurrect the tour", () => {
    const skipped = advanceTutorial({ ...createTutorialState(), step: "create-submit" }, { type: "skip" });
    expect(skipped.step).toBe("inactive");
    expect(advanceTutorial(skipped, { type: "archive-created", archiveId: "a" }).step).toBe("inactive");
  });
  it("old, malformed, completed and skipped state never triggers first-use", () => {
    expect(shouldOfferTutorial({ status: "pending", seenTips: [] })).toBe(true);
    for (const value of [{ status: "legacy" }, { status: "completed" }, { status: "skipped" }, { status: "invalid" }, null]) {
      expect(shouldOfferTutorial(normalizeTutorialProgress(value))).toBe(false);
    }
    expect(normalizeTutorialProgress({ status: "completed", seenTips: ["registry", "secret", "registry"] }).seenTips).toEqual(["registry"]);
  });
  it("a fresh browser remembers completion and does not write cloud settings", async () => {
    const storage = new Map<string, string>();
    vi.stubGlobal("localStorage", { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value) });
    expect((await loadTutorialProgress(false)).status).toBe("pending");
    await saveTutorialProgress({ status: "completed", seenTips: ["categories"] });
    expect(await loadTutorialProgress(false)).toEqual({ status: "completed", seenTips: ["categories"] });
    expect([...storage.keys()]).toEqual(["chronicle.onboarding.local.v1"]);
  });
  it("existing settings or archives suppress automatic welcome", async () => {
    for (const oldSettings of [true, false]) {
      const storage = new Map<string, string>(oldSettings ? [["chronicle.app-settings.v2", "{}"]] : []);
      vi.stubGlobal("localStorage", { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value) });
      expect((await loadTutorialProgress(!oldSettings)).status).toBe("legacy");
    }
  });
  it("snapshot success from another archive cannot move the tutorial", () => {
    const state = { ...createTutorialState(), step: "snapshot" as const, archiveId: "a" };
    expect(advanceTutorial(state, { type: "snapshot-created", archiveId: "b" }).step).toBe("snapshot");
  });
});
