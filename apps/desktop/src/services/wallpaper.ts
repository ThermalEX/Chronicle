import { invoke, isTauri } from "@tauri-apps/api/core";
import { ref } from "vue";
import { t } from "./i18n";

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

export function wallpaperWarningLabel(warning?: string | null): string {
  if (warning === "preferences-damaged") return t("本机壁纸设置已损坏，已恢复主题纯色。");
  if (warning === "image-unreadable") return t("本机壁纸图片无法读取，已恢复主题纯色。");
  if (warning === "image-invalid") return t("本机壁纸文件名无效，已恢复主题纯色。");
  return warning ?? "";
}

export function applyWallpaper(state: WallpaperState): void {
  currentWallpaper.value = { ...state };
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.dataset.wallpaperMode = state.mode === "image" && state.imageDataUrl ? "image" : "color";
  root.style.setProperty("--wallpaper-opacity", `${100 - state.transparency}%`);
  root.style.setProperty("--wallpaper-blur", `${state.blurPx}px`);
}

export async function loadLocalWallpaper(): Promise<void> {
  if (!isTauri()) return;
  applyWallpaper(await invoke<WallpaperState>("load_local_wallpaper"));
}

export async function previewLocalWallpaper(path: string): Promise<string> {
  if (!isTauri()) throw new Error("Image wallpaper preview requires the desktop app");
  return invoke<string>("preview_local_wallpaper", { path });
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

export async function saveLocalWallpaper(draft: WallpaperSaveRequest): Promise<void> {
  if (!isTauri()) throw new Error("Image wallpaper can only be saved in the desktop app");
  applyWallpaper(await invoke<WallpaperState>("save_local_wallpaper", { request: draft }));
}
