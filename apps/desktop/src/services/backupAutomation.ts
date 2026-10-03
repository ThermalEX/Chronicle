import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n";
export type BackupTriggerConfig = {
  mode: "file_change" | "game_exit";
  executablePath: string | null;
  quietSeconds: number;
};
export type BackupRuntimeStatus = {
  entryId: string;
  generation: number;
  status: string;
  reasonCode: string | null;
};
export const defaultBackupTrigger = (): BackupTriggerConfig => ({
  mode: "file_change",
  executablePath: null,
  quietSeconds: 5,
});
export const backupAutomationSupported = () =>
  isTauri() && navigator.userAgent.includes("Windows");
export function validateBackupTrigger(
  config: BackupTriggerConfig,
): string | null {
  if (
    !Number.isInteger(config.quietSeconds) ||
    config.quietSeconds < 1 ||
    config.quietSeconds > 300
  )
    return "backup_invalid_quiet_seconds";
  if (config.mode === "game_exit") {
    if (!config.executablePath) return "backup_executable_required";
    if (!/^(?:[a-z]:[\\/]|\\\\).+\.exe$/i.test(config.executablePath))
      return "backup_invalid_executable";
  }
  return null;
}
export const getBackupTrigger = (entryId: string) =>
  invoke<BackupTriggerConfig>("get_backup_trigger", { entryId });
export const setBackupTrigger = (
  entryId: string,
  config: BackupTriggerConfig,
) => invoke<void>("set_backup_trigger", { entryId, config });
export const listBackupProcesses = async () => {
  const result = await invoke<{
    processes: {
      pid: number;
      startTime: number;
      name: string;
      executablePath: string;
    }[];
    complete: boolean;
    partial: boolean;
  }>("list_backup_processes");
  const seen = new Set<string>();
  result.processes.sort((a, b) => b.startTime - a.startTime || b.pid - a.pid);
  result.processes = result.processes.filter((process) => {
    if (seen.has(process.executablePath)) return false;
    seen.add(process.executablePath);
    return true;
  });
  return result;
};
export const getBackupRuntimeStates = () =>
  invoke<BackupRuntimeStatus[]>("get_backup_runtime_states");
export const subscribeBackupRuntime = (
  callback: (status: BackupRuntimeStatus) => void,
) =>
  listen<BackupRuntimeStatus>("backup-automation-state", (event) =>
    callback(event.payload),
  );
export const acceptBackupRuntime = (
  current: BackupRuntimeStatus | undefined,
  incoming: BackupRuntimeStatus,
) => (current && incoming.generation < current.generation ? current : incoming);
export function backupAutomationLabel(code: string): string {
  const labels: Record<string, string> = {
    waiting: "等待游戏启动",
    running: "游戏运行中",
    settling: "等待存档写入完成",
    backing_up: "正在备份",
    unchanged: "内容未变化，已跳过备份",
    needs_attention: "自动备份需要处理",
    backup_executable_required: "请选择游戏程序",
    backup_invalid_executable: "请选择完整的 .exe 路径",
    backup_executable_unavailable: "游戏程序不可访问，请重新选择",
    backup_invalid_quiet_seconds: "静默时间必须为 1–300 秒",
    backup_config_corrupt: "自动备份配置损坏，请检查本机配置",
    backup_process_unknown: "无法确认游戏是否退出，暂不备份",
    backup_settle_timeout: "存档持续写入，已停止本次自动备份",
    backup_capture_failed: "自动备份失败，请手动重试",
  };
  return t(labels[code] ?? code);
}
