import { isTauri } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

export async function notifyTrayBackground(enabled: boolean): Promise<void> {
  if (!enabled || !isTauri()) return;

  const granted = await isPermissionGranted();
  const permission = granted ? "granted" : await requestPermission();
  if (permission !== "granted") return;

  sendNotification({
    title: "Chronicle 正在后台运行",
    body: "已最小化到托盘，自动备份会继续执行。",
  });
}
