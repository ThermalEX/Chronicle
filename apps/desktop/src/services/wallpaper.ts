import { invoke, isTauri } from "@tauri-apps/api/core";
import { ref } from "vue";

export interface WallpaperState {
  mode: "color" | "image";
  transparency: number;
  blurPx: number;
  imageDataUrl?: string | null;
  warning?: string | null;
}

export interface WallpaperSaveRequest {
  mode: WallpaperState["mode"];
  transparency: number;
  blurPx: number;
  sourcePath?: string | null;
  removeImage: boolean;
}

export const currentWallpaper = ref<WallpaperState>({ mode: "color", transparency: 28, blurPx: 12 });

export function applyWallpaper(state: WallpaperState): void {
  currentWallpaper.value = { ...state };
  const root = document.documentElement;
  root.dataset.wallpaperMode = state.mode === "image" && state.imageDataUrl ? "image" : "color";
  root.style.setProperty("--wallpaper-opacity", `${100 - state.transparency}%`);
  root.style.setProperty("--wallpaper-blur", `${state.blurPx}px`);
  if (state.imageDataUrl) root.style.setProperty("--wallpaper-image", `url("${state.imageDataUrl}")`);
  else root.style.removeProperty("--wallpaper-image");
}

export async function loadLocalWallpaper(): Promise<void> {
  if (!isTauri()) return;
  applyWallpaper(await invoke<WallpaperState>("load_local_wallpaper"));
}

export async function previewLocalWallpaper(path: string): Promise<string> {
  if (!isTauri()) throw new Error("Image wallpaper preview requires the desktop app");
  return invoke<string>("preview_local_wallpaper", { path });
}

export async function saveLocalWallpaper(draft: WallpaperSaveRequest): Promise<void> {
  if (!isTauri()) throw new Error("Image wallpaper can only be saved in the desktop app");
  applyWallpaper(await invoke<WallpaperState>("save_local_wallpaper", { request: draft }));
}
