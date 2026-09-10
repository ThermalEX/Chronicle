import { beforeEach, describe, expect, it, vi } from "vitest";

const isTauri = vi.fn();
const isPermissionGranted = vi.fn();
const requestPermission = vi.fn();
const sendNotification = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ isTauri }));
vi.mock("@tauri-apps/plugin-notification", () => ({ isPermissionGranted, requestPermission, sendNotification }));

describe("tray notification", () => {
  beforeEach(() => {
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
});
