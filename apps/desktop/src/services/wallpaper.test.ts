import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: vi.fn(() => true),
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { applyWallpaper, currentWallpaper, loadLocalWallpaper, saveLocalWallpaper } from "./wallpaper";
import { defaultAppSettings } from "./settings";

afterEach(() => vi.unstubAllGlobals());

describe("local wallpaper", () => {
  it("defaults to color and never enters cloud application settings", () => {
    expect(currentWallpaper.value.mode).toBe("color");
    expect(defaultAppSettings).not.toHaveProperty("wallpaper");
    expect(defaultAppSettings).not.toHaveProperty("imageDataUrl");
  });

  it("applies only wallpaper CSS and retains image when returning to color", () => {
    const root = { dataset: {} as Record<string, string>, style: { setProperty: vi.fn(), removeProperty: vi.fn() } };
    vi.stubGlobal("document", { documentElement: root });
    applyWallpaper({ mode: "image", transparency: 28, blurPx: 12, imageDataUrl: "data:image/png;base64,AA==" });
    expect(root.dataset.wallpaperMode).toBe("image");
    expect(root.style.setProperty).toHaveBeenCalledWith("--wallpaper-image", expect.stringContaining("data:image/png"));
    applyWallpaper({ ...currentWallpaper.value, mode: "color" });
    expect(root.dataset.wallpaperMode).toBe("color");
    expect(currentWallpaper.value.imageDataUrl).toContain("data:image/png");
  });

  it("loads and saves through the local Tauri commands", async () => {
    vi.stubGlobal("document", { documentElement: { dataset: {}, style: { setProperty: vi.fn(), removeProperty: vi.fn() } } });
    const stored = { mode: "image" as const, transparency: 15, blurPx: 5, imageDataUrl: "data:image/png;base64,AA==" };
    vi.mocked(invoke).mockResolvedValue(stored);
    await loadLocalWallpaper();
    expect(currentWallpaper.value).toEqual(stored);
    await saveLocalWallpaper({ mode: "color", transparency: 15, blurPx: 5, removeImage: false });
    expect(invoke).toHaveBeenCalledWith("save_local_wallpaper", { request: expect.objectContaining({ mode: "color" }) });
  });
});
