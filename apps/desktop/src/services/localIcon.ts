import { invoke, isTauri } from "@tauri-apps/api/core";
import { computed, ref } from "vue";
import defaultIcon from "../assets/icon.png";
import type { ColorMode, ColorTheme } from "./appearance";
import type { SoundSaveRequest, ThemeSoundPreview } from "./themeSounds";

export interface LocalIconState { mode: "color" | "image"; imageDataUrl?: string | null; warning?: string | null }
export interface LocalIconRequest { mode: "color" | "image"; sourcePath?: string; removeImage: boolean }
export const localIcon = ref<LocalIconState>({ mode: "color" });
export const currentAppIcon = computed(() => localIcon.value.mode === "image" && localIcon.value.imageDataUrl ? localIcon.value.imageDataUrl : defaultIcon);
export async function loadLocalIcon(): Promise<void> {
  if (isTauri()) localIcon.value = await invoke<LocalIconState>("load_local_icon");
}
export async function saveLocalIcon(request: LocalIconRequest): Promise<void> {
  localIcon.value = await invoke<LocalIconState>("save_local_icon", { request });
}
export interface LocalThemePack {
  colorTheme: ColorTheme; customAccent: string; colorMode: ColorMode; transparency: number; blurPx: number;
  iconPath: string; wallpaperPath: string; iconDataUrl: string; wallpaperDataUrl: string;
  sounds?: ThemeSoundPreview | null;
}
export interface ThemeExportRequest {
  colorTheme: ColorTheme; customAccent: string; colorMode: ColorMode; transparency: number; blurPx: number;
  iconMode: "color" | "image"; iconSourcePath?: string; wallpaperMode: "color" | "image"; wallpaperSourcePath?: string;
  sounds?: SoundSaveRequest;
}
export async function exportLocalTheme(directory: string, request: ThemeExportRequest): Promise<string> {
  return invoke<string>("export_local_theme", { directory, request });
}
export async function previewLocalTheme(path: string): Promise<LocalThemePack> {
  return invoke<LocalThemePack>("preview_local_theme", { path });
}
