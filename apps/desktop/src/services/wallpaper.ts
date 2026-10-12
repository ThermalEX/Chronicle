import { invoke, isTauri } from "@tauri-apps/api/core";
import { ref } from "vue";

export interface WallpaperState {
  mode: "color" | "image";
  transparency: number;
  blurPx: number;
  imageDataUrl?: string | null;
  warning?: string | null;
}

export const currentWallpaper = ref<WallpaperState>({ mode: "color", transparency: 28, blurPx: 12 });

export function applyWallpaper(state: WallpaperState): void {
  currentWallpaper.value = { ...state };
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.dataset.wallpaperMode = state.mode === "image" && state.imageDataUrl ? "image" : "color";
  root.style.setProperty("--wallpaper-opacity", `${100 - state.transparency}%`);
  root.style.setProperty("--wallpaper-blur", `${state.blurPx}px`);
}

// Keep three recent images cached, but never revoke a player's current/fading/loading resource.
interface WallpaperResource { pending: Promise<string>; users: number; url?: string }
const wallpaperUrls = new Map<string, WallpaperResource>();
function trimWallpaperUrls(): void {
  for (const [path, resource] of wallpaperUrls) {
    if (wallpaperUrls.size <= 3) break;
    if (resource.users || !resource.url) continue;
    wallpaperUrls.delete(path);URL.revokeObjectURL(resource.url);
  }
}
export function releaseWallpaperImageUrl(url: string): void {
  for (const resource of wallpaperUrls.values()) {
    if (resource.url === url) { resource.users = Math.max(0, resource.users - 1); break; }
  }
  trimWallpaperUrls();
}
export async function loadWallpaperImageUrl(path: string): Promise<string> {
  if (!isTauri()) throw new Error("Image wallpaper preview requires the desktop app");
  let resource = wallpaperUrls.get(path);
  if (resource) { resource.users++;wallpaperUrls.delete(path);wallpaperUrls.set(path, resource);return resource.pending; }
  const pending = invoke<ArrayBuffer>("load_wallpaper_image", { path }).then(bytes => {
    const url = URL.createObjectURL(new Blob([bytes]));resource!.url = url;return url;
  });
  resource = { pending, users: 1 };wallpaperUrls.set(path, resource);
  void pending.catch(() => { if (wallpaperUrls.get(path) === resource) wallpaperUrls.delete(path); });
  trimWallpaperUrls();return pending;
}
