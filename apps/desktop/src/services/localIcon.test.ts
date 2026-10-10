import { afterEach, expect, it, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";
import { currentAppIcon, loadLocalIcon, saveLocalIcon } from "./localIcon";
import defaultIcon from "../assets/icon.png";
afterEach(() => vi.resetAllMocks());
it("uses saved custom icon but falls back to the Chronicle icon when unavailable", async () => {
  vi.mocked(invoke).mockResolvedValue({ mode: "image", imageDataUrl: "data:image/png;base64,AA==" });
  await loadLocalIcon();
  expect(currentAppIcon.value).toBe("data:image/png;base64,AA==");
  vi.mocked(invoke).mockResolvedValue({ mode: "color", warning: "image-unreadable" });
  await loadLocalIcon();
  expect(currentAppIcon.value).toBe(defaultIcon);
});
it("does not change the displayed icon after a failed save", async () => {
  const previous = currentAppIcon.value;
  vi.mocked(invoke).mockRejectedValue(new Error("disk full"));
  await expect(saveLocalIcon({ mode: "image", sourcePath: "custom.png", removeImage: false })).rejects.toThrow("disk full");
  expect(currentAppIcon.value).toBe(previous);
});
