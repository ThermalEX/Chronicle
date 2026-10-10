import { invoke, isTauri } from "@tauri-apps/api/core";
import { ref } from "vue";

export const soundEvents = ["connected", "disconnected", "connectionFailed", "notification", "default"] as const;
export type SoundEvent = typeof soundEvents[number];
export interface SoundAsset { name: string; dataUrl: string }
export interface SoundState { enabled: boolean; volume: number; files: Partial<Record<SoundEvent, SoundAsset>>; warning?: string | null }
export interface SoundSaveRequest { enabled: boolean; volume: number; sourcePaths: Partial<Record<SoundEvent, string | null>> }
export interface ThemeSoundPreview extends SoundState { sourcePaths: Partial<Record<SoundEvent, string>> }
export const localSounds = ref<SoundState>({ enabled: false, volume: 60, files: {} });
let activeAudio: HTMLAudioElement | undefined;
let activePreview = false;
let lastAutomatic = -Infinity;

export async function loadLocalSounds(): Promise<void> {
  if (isTauri()) localSounds.value = await invoke<SoundState>("load_local_sounds");
}
export async function saveLocalSounds(request: SoundSaveRequest): Promise<void> {
  const saved = await invoke<SoundState>("save_local_sounds", { request });
  stopThemeSounds();
  localSounds.value = saved;
}
export async function previewLocalSound(path: string): Promise<SoundAsset> {
  return invoke<SoundAsset>("preview_local_sound", { path });
}

export function stopThemeSounds(): void {
  activeAudio?.pause();
  activeAudio = undefined;
  activePreview = false;
}
export function stopSoundPreview(): void { if (activePreview) stopThemeSounds(); }

async function playAsset(asset: SoundAsset | undefined, volume: number, preview: boolean): Promise<boolean> {
  if (!asset?.dataUrl || !Number.isFinite(volume) || volume <= 0 || typeof Audio === "undefined") return false;
  const now = performance.now();
  if (!preview && (now - lastAutomatic < 2000 || (activeAudio && !activeAudio.paused && !activeAudio.ended))) return false;
  stopThemeSounds();
  let audio: HTMLAudioElement | undefined;
  try {
    audio = new Audio(asset.dataUrl);
    audio.volume = Math.min(100, volume) / 100;
    activeAudio = audio;
    activePreview = preview;
    if (!preview) lastAutomatic = now;
    await audio.play();
    return true;
  } catch {
    if (activeAudio === audio) stopThemeSounds();
    return false;
  }
}
export function playThemeSound(event: SoundEvent): Promise<boolean> {
  const state = localSounds.value;
  return state.enabled ? playAsset(state.files[event], state.volume, false) : Promise.resolve(false);
}
export function previewThemeSound(asset: SoundAsset | undefined, volume: number): Promise<boolean> {
  return playAsset(asset, volume, true);
}
