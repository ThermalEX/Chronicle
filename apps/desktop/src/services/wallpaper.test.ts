import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: vi.fn(() => true),
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { applyWallpaper, currentWallpaper, loadLocalWallpaper, saveLocalWallpaper, loadWallpaperImageUrl, releaseWallpaperImageUrl } from "./wallpaper";
import { defaultAppSettings } from "./settings";

afterEach(() => vi.unstubAllGlobals());

describe("local wallpaper", () => {
  it("loads background bytes without Base64 and shares requests for the same image", async () => {
    const createObjectURL=vi.fn((_blob:Blob)=>"blob:wallpaper");vi.stubGlobal("URL",{createObjectURL,revokeObjectURL:vi.fn()});
    const bytes=new Uint8Array([137,80,78,71]).buffer;vi.mocked(invoke).mockReset().mockResolvedValue(bytes);
    expect(await loadWallpaperImageUrl("startup.png")).toBe("blob:wallpaper");
    expect(await loadWallpaperImageUrl("startup.png")).toBe("blob:wallpaper");
    expect(invoke).toHaveBeenCalledTimes(1);expect(invoke).toHaveBeenCalledWith("load_wallpaper_image",{path:"startup.png"});
    expect(createObjectURL.mock.calls[0]?.[0]).toBeInstanceOf(Blob);
    releaseWallpaperImageUrl("blob:wallpaper");releaseWallpaperImageUrl("blob:wallpaper");
  });
  it("keeps the saved image alive while settings previews load other images", async () => {
    let sequence=0;const revoked:string[]=[];vi.stubGlobal("URL",{createObjectURL:()=>`blob:pin-${++sequence}`,revokeObjectURL:(url:string)=>revoked.push(url)});
    vi.mocked(invoke).mockResolvedValue(new ArrayBuffer(4));
    const saved=await loadWallpaperImageUrl("saved-A.png");
    for(const path of ["preview-B.png","preview-C.png","preview-D.png"]){const url=await loadWallpaperImageUrl(path);releaseWallpaperImageUrl(url);}
    expect(revoked).not.toContain(saved);expect(await loadWallpaperImageUrl("saved-A.png")).toBe(saved);
    releaseWallpaperImageUrl(saved);releaseWallpaperImageUrl(saved);
  });
  it("never revokes an image before an in-flight loader acquires it", async () => {
    let sequence=0;const revoked:string[]=[];vi.stubGlobal("URL",{createObjectURL:()=>`blob:pending-${++sequence}`,revokeObjectURL:(url:string)=>revoked.push(url)});
    let finish!:(value:ArrayBuffer)=>void;vi.mocked(invoke).mockImplementation(async(_command,args:any)=>args.path==="slow-A.png"?await new Promise<ArrayBuffer>(resolve=>{finish=resolve;}):new ArrayBuffer(4));
    const pending=loadWallpaperImageUrl("slow-A.png");
    for(const path of ["pending-B.png","pending-C.png","pending-D.png"]){const url=await loadWallpaperImageUrl(path);releaseWallpaperImageUrl(url);}
    finish(new ArrayBuffer(4));const url=await pending;expect(revoked).not.toContain(url);releaseWallpaperImageUrl(url);
  });
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
    expect(currentWallpaper.value.imageDataUrl).toContain("data:image/png");
    expect(root.style.setProperty).toHaveBeenCalledWith("--wallpaper-opacity", "72%");
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
