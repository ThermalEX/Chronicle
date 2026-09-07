import { invoke, isTauri } from "@tauri-apps/api/core";
import { reactive } from "vue";

export type CloseBehavior = "ask" | "tray" | "exit";
export type BackupSchedule = "off" | "15m" | "1h" | "6h" | "daily";
export type SyncDirection = "bidirectional" | "upload" | "download";
export type ConflictStrategy = "ask" | "newest" | "local" | "remote";

export interface AppSettings {
  launchAtStartup: boolean;
  closeBehavior: CloseBehavior;
  checkForUpdates: boolean;
  notifications: boolean;
  createInitialSnapshot: boolean;
  backupSchedule: BackupSchedule;
  retentionCount: number | null;
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
  maxConcurrentMetadataReads: number;
  maxConcurrentTransfers: number;
  requestDelayMs: number;
  retryLimit: number;
}

const APP_SETTINGS_KEY = "chronicle.app-settings.v1";
const CLOUD_SETTINGS_KEY = "chronicle.cloud-settings.v1";

export interface SettingsDocument {
  formatVersion: 1;
  app: AppSettings;
  cloud: CloudSettings;
}

const defaultAppSettings: AppSettings = {
  launchAtStartup: false,
  closeBehavior: "ask",
  checkForUpdates: true,
  notifications: true,
  createInitialSnapshot: true,
  backupSchedule: "off",
  retentionCount: null,
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
  maxConcurrentMetadataReads: 2,
  maxConcurrentTransfers: 2,
  requestDelayMs: 150,
  retryLimit: 5,
};

function loadSettings<T extends object>(key: string, defaults: T): T {
  try {
    const saved = localStorage.getItem(key);
    return saved ? { ...defaults, ...JSON.parse(saved) as Partial<T> } : { ...defaults };
  } catch {
    return { ...defaults };
  }
}

export const appSettings = reactive({ ...defaultAppSettings });
export const cloudSettings = reactive({ ...defaultCloudSettings });

function settingsDocument(): SettingsDocument {
  return {
    formatVersion: 1,
    app: { ...appSettings },
    cloud: { ...cloudSettings },
  };
}

export async function initializeSettings(): Promise<void> {
  if (isTauri()) {
    const saved = await invoke<Partial<SettingsDocument>>("load_settings");
    Object.assign(appSettings, defaultAppSettings, saved.app ?? {});
    Object.assign(cloudSettings, defaultCloudSettings, saved.cloud ?? {});
    return;
  }
  Object.assign(appSettings, loadSettings(APP_SETTINGS_KEY, defaultAppSettings));
  Object.assign(cloudSettings, loadSettings(CLOUD_SETTINGS_KEY, defaultCloudSettings));
}

export async function persistSettings(): Promise<void> {
  if (isTauri()) {
    await invoke("save_settings", { settings: settingsDocument() });
    return;
  }
  localStorage.setItem(APP_SETTINGS_KEY, JSON.stringify(appSettings));
  localStorage.setItem(CLOUD_SETTINGS_KEY, JSON.stringify(cloudSettings));
}

export async function saveAppSettings(value: AppSettings): Promise<void> {
  Object.assign(appSettings, value);
  await persistSettings();
}

export async function saveCloudSettings(value: CloudSettings): Promise<void> {
  Object.assign(cloudSettings, value);
  await persistSettings();
}

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
