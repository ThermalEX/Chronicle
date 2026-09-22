import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLocale } from "./i18n";

const isTauri = vi.fn();
const isPermissionGranted = vi.fn();
const requestPermission = vi.fn();
const sendNotification = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ isTauri }));
vi.mock("@tauri-apps/plugin-notification", () => ({ isPermissionGranted, requestPermission, sendNotification }));

describe("tray notification", () => {
  beforeEach(() => {
    setLocale("zh-CN");
    isTauri.mockReset().mockReturnValue(true);
    isPermissionGranted.mockReset().mockResolvedValue(true);
    requestPermission.mockReset();
    sendNotification.mockReset();
  });

  it("notifies after Chronicle is hidden to the tray when desktop notifications are enabled", async () => {
    const { notifyTrayBackground } = await import("./trayNotification");

    await notifyTrayBackground(true);

    expect(sendNotification).toHaveBeenCalledWith({ title: "Chronicle 正在后台运行", body: "已最小化到托盘，自动备份会继续执行。" });
  });

  it("does not notify when desktop notifications are disabled", async () => {
    const { notifyTrayBackground } = await import("./trayNotification");

    await notifyTrayBackground(false);

    expect(sendNotification).not.toHaveBeenCalled();
  });

  it("sends the selected language to desktop notifications", async () => {
    const { notifyTrayBackground } = await import("./trayNotification");
    try {
      setLocale("en");
      await notifyTrayBackground(true);
      expect(sendNotification).toHaveBeenCalledWith({ title: "Chronicle is running in the background", body: "Minimized to the tray. Automatic backups will continue." });
    } finally { setLocale("zh-CN"); }
  });
});
