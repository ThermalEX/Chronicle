import { afterEach, beforeEach, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";

let players: FakeAudio[];
class FakeAudio {
  volume = 1;
  paused = true;
  ended = false;
  onended: (() => void) | null = null;
  constructor(public src: string) { players.push(this); }
  async play() { this.paused = false; }
  pause() { this.paused = true; }
}
beforeEach(() => { vi.resetModules(); vi.useFakeTimers(); players = []; vi.stubGlobal("Audio", FakeAudio); vi.mocked(invoke).mockReset(); });
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

const configured = { enabled: true, volume: 35, files: {
  connected: { name: "设备连接.WAV", dataUrl: "data:audio/wav;base64,connected" },
  notification: { name: "通知.WAV", dataUrl: "data:audio/wav;base64,notification" },
} };

it("stays silent with no theme, with a disabled theme or with zero volume", async () => {
  const sounds = await import("./themeSounds");
  expect(await sounds.playThemeSound("connected")).toBe(false);
  sounds.localSounds.value = { ...configured, enabled: false };
  expect(await sounds.playThemeSound("connected")).toBe(false);
  sounds.localSounds.value = { ...configured, volume: 0 };
  expect(await sounds.playThemeSound("connected")).toBe(false);
  expect(players).toHaveLength(0);
});

it("uses the selected event and volume, and does not fall back for missing events", async () => {
  const sounds = await import("./themeSounds");
  sounds.localSounds.value = configured;
  expect(await sounds.playThemeSound("notification")).toBe(true);
  expect(players[0]?.src).toBe("data:audio/wav;base64,notification");
  expect(players[0]?.volume).toBe(.35);
  expect(await sounds.playThemeSound("disconnected")).toBe(false);
});

it("limits automatic bursts without queueing old notifications", async () => {
  const sounds = await import("./themeSounds");
  sounds.localSounds.value = configured;
  expect(await sounds.playThemeSound("connected")).toBe(true);
  expect(await sounds.playThemeSound("notification")).toBe(false);
  players[0]!.ended = true;
  players[0]!.paused = true;
  vi.advanceTimersByTime(1000);
  expect(await sounds.playThemeSound("notification")).toBe(false);
  vi.advanceTimersByTime(1000);
  expect(await sounds.playThemeSound("notification")).toBe(true);
  expect(players).toHaveLength(2);
});

it("previews an unsaved sound while disabled and replaces the previous preview", async () => {
  const sounds = await import("./themeSounds");
  expect(await sounds.previewThemeSound(configured.files.connected, 60)).toBe(true);
  expect(await sounds.previewThemeSound(configured.files.notification, 20)).toBe(true);
  expect(players[0]?.paused).toBe(true);
  expect(players[1]?.volume).toBe(.2);
  expect(sounds.localSounds.value.enabled).toBe(false);
  sounds.stopSoundPreview();
  expect(players[1]?.paused).toBe(true);
});

it("isolates playback failures from the operation that reported them", async () => {
  vi.spyOn(FakeAudio.prototype, "play").mockRejectedValue(new Error("Playback denied"));
  const sounds = await import("./themeSounds");
  sounds.localSounds.value = configured;
  expect(await sounds.playThemeSound("connected")).toBe(false);
  expect(players[0]?.paused).toBe(true);
});

it("only replaces saved sounds after persistence succeeds", async () => {
  const sounds = await import("./themeSounds");
  sounds.localSounds.value = configured;
  vi.mocked(invoke).mockRejectedValueOnce(new Error("Disk full"));
  await expect(sounds.saveLocalSounds({ enabled: false, volume: 60, sourcePaths: {} })).rejects.toThrow("Disk full");
  expect(sounds.localSounds.value.enabled).toBe(true);
  vi.mocked(invoke).mockResolvedValueOnce({ enabled: false, volume: 60, files: {} });
  await sounds.saveLocalSounds({ enabled: false, volume: 60, sourcePaths: {} });
  expect(sounds.localSounds.value.enabled).toBe(false);
});
