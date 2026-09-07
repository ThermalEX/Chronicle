import { reactive, watch } from "vue";

export type CloseBehavior = "ask" | "tray" | "exit";
export type BackupSchedule = "off" | "15m" | "1h" | "6h" | "daily";
export type SyncDirection = "bidirectional" | "upload" | "download";
export type ConflictStrategy = "ask" | "newest" | "local" | "remote";

export interface AppSettings {
  launchAtStartup: boolean;
  closeBehavior: CloseBehavior;
  checkForUpdates: boolean;
  notifications: boolean;
  defaultCategory: string;
  createInitialSnapshot: boolean;
  backupSchedule: BackupSchedule;
  retentionCount: number;
  searchShortcut: string;
  snapshotShortcut: string;
  settingsShortcut: string;
}

export interface CloudSettings {
  enabled: boolean;
  endpoint: string;
  username: string;
  remotePath: string;
  syncDirection: SyncDirection;
  conflictStrategy: ConflictStrategy;
  syncOnLaunch: boolean;
}

const APP_SETTINGS_KEY = "chronicle.app-settings.v1";
const CLOUD_SETTINGS_KEY = "chronicle.cloud-settings.v1";

const defaultAppSettings: AppSettings = {
  launchAtStartup: false,
  closeBehavior: "ask",
  checkForUpdates: true,
  notifications: true,
  defaultCategory: "未分类",
  createInitialSnapshot: true,
  backupSchedule: "off",
  retentionCount: 30,
  searchShortcut: "Ctrl+K",
  snapshotShortcut: "Ctrl+Shift+B",
  settingsShortcut: "Ctrl+,",
};

const defaultCloudSettings: CloudSettings = {
  enabled: false,
  endpoint: "",
  username: "",
  remotePath: "/Chronicle",
  syncDirection: "bidirectional",
  conflictStrategy: "ask",
  syncOnLaunch: true,
};

function loadSettings<T extends object>(key: string, defaults: T): T {
  try {
    const saved = localStorage.getItem(key);
    return saved ? { ...defaults, ...JSON.parse(saved) as Partial<T> } : { ...defaults };
  } catch {
    return { ...defaults };
  }
}

export const appSettings = reactive(loadSettings(APP_SETTINGS_KEY, defaultAppSettings));
export const cloudSettings = reactive(loadSettings(CLOUD_SETTINGS_KEY, defaultCloudSettings));

watch(appSettings, (value) => localStorage.setItem(APP_SETTINGS_KEY, JSON.stringify(value)), { deep: true });
watch(cloudSettings, (value) => localStorage.setItem(CLOUD_SETTINGS_KEY, JSON.stringify(value)), { deep: true });

export function resetAppSettings(): void {
  Object.assign(appSettings, defaultAppSettings);
}

export function shortcutMatches(event: KeyboardEvent, shortcut: string): boolean {
  const parts = shortcut.toLocaleLowerCase().split("+").map((part) => part.trim());
  const key = parts.at(-1);
  return Boolean(
    key && event.key.toLocaleLowerCase() === key &&
    event.ctrlKey === parts.includes("ctrl") &&
    event.shiftKey === parts.includes("shift") &&
    event.altKey === parts.includes("alt") &&
    event.metaKey === parts.includes("meta"),
  );
}
