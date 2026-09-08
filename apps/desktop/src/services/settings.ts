import { invoke, isTauri } from "@tauri-apps/api/core";
import { reactive } from "vue";
import { normalizeAppearance, type ColorMode, type ColorTheme } from "./appearance";

export type CloseBehavior = "ask" | "tray" | "exit";
export type BackupSchedule = "off" | "15m" | "1h" | "6h" | "daily";
export type CloudProvider = "webdav" | "github";

export interface AppSettings {
  colorTheme: ColorTheme;
  colorMode: ColorMode;
  launchAtStartup: boolean;
  closeBehavior: CloseBehavior;
  checkForUpdates: boolean;
  notifications: boolean;
  createInitialSnapshot: boolean;
  backupSchedule: BackupSchedule;
  retentionCount: number | null;
  recycleBinEnabled: boolean;
  recycleBinPath: string;
  searchShortcut: string;
  snapshotShortcut: string;
  settingsShortcut: string;
}

export interface CloudSource {
  id: string;
  name: string;
  provider: CloudProvider;
  endpoint: string;
  username: string;
  remotePath: string;
  credentialRef: string;
  repository?: string;
  branch?: string;
}

export interface CloudSettings {
  enabled: boolean;
  activeSourceId: string | null;
  sources: CloudSource[];
  maxConcurrentMetadataReads: number;
  maxConcurrentTransfers: number;
  requestDelayMs: number;
  retryLimit: number;
}

const APP_SETTINGS_KEY = "chronicle.app-settings.v2";
const CLOUD_SETTINGS_KEY = "chronicle.cloud-settings.v2";

export interface SettingsDocument {
  formatVersion: 2;
  app: AppSettings;
  cloud: CloudSettings;
}

const defaultAppSettings: AppSettings = {
  colorTheme: "teal",
  colorMode: "light",
  launchAtStartup: false,
  closeBehavior: "ask",
  checkForUpdates: true,
  notifications: true,
  createInitialSnapshot: true,
  backupSchedule: "off",
  retentionCount: null,
  recycleBinEnabled: true,
  recycleBinPath: "",
  searchShortcut: "Ctrl+K",
  snapshotShortcut: "Ctrl+Shift+B",
  settingsShortcut: "Ctrl+,",
};

const defaultCloudSettings: CloudSettings = {
  enabled: false,
  activeSourceId: null,
  sources: [],
  maxConcurrentMetadataReads: 2,
  maxConcurrentTransfers: 2,
  requestDelayMs: 150,
  retryLimit: 5,
};

type LegacyCloudSettings = Partial<CloudSettings> & {
  endpoint?: string;
  username?: string;
  remotePath?: string;
};

function normalizedCloud(value?: LegacyCloudSettings): CloudSettings {
  const raw = value ?? {};
  let sources = Array.isArray(raw.sources) ? raw.sources.map((source) => ({ ...source })) : [];
  let activeSourceId = raw.activeSourceId ?? null;
  if (!sources.length && raw.endpoint) {
    const id = crypto.randomUUID();
    sources = [{
      id,
      name: "WebDAV",
      provider: "webdav",
      endpoint: raw.endpoint,
      username: raw.username ?? "",
      remotePath: raw.remotePath || "/Chronicle",
      credentialRef: `chronicle-webdav:${id}`,
    }];
    activeSourceId = id;
  }
  if (!sources.some((source) => source.id === activeSourceId)) activeSourceId = sources[0]?.id ?? null;
  return {
    enabled: Boolean(raw.enabled && sources.length),
    activeSourceId,
    sources,
    maxConcurrentMetadataReads: Math.max(1, Math.min(4, Number(raw.maxConcurrentMetadataReads) || 2)),
    maxConcurrentTransfers: Math.max(1, Math.min(4, Number(raw.maxConcurrentTransfers) || 2)),
    requestDelayMs: Math.max(0, Math.min(5000, Number(raw.requestDelayMs) || 150)),
    retryLimit: Math.max(1, Math.min(10, Number(raw.retryLimit) || 5)),
  };
}

function loadSettings<T extends object>(key: string, defaults: T): T {
  try {
    const saved = localStorage.getItem(key);
    return saved ? { ...defaults, ...JSON.parse(saved) as Partial<T> } : { ...defaults };
  } catch {
    return { ...defaults };
  }
}

export const appSettings = reactive({ ...defaultAppSettings });
export const cloudSettings = reactive<CloudSettings>(normalizedCloud());

function settingsDocument(): SettingsDocument {
  return {
    formatVersion: 2,
    app: { ...appSettings },
    cloud: { ...cloudSettings, sources: cloudSettings.sources.map((source) => ({ ...source })) },
  };
}

export async function initializeSettings(): Promise<void> {
  if (isTauri()) {
    const saved = await invoke<Partial<SettingsDocument> & { cloud?: LegacyCloudSettings }>("load_settings");
    Object.assign(appSettings, defaultAppSettings, saved.app ?? {}, normalizeAppearance(saved.app ?? {}));
    Object.assign(cloudSettings, normalizedCloud(saved.cloud));
    if (saved.formatVersion !== 2) await persistSettings();
    return;
  }
  const savedApp = loadSettings(APP_SETTINGS_KEY, defaultAppSettings);
  Object.assign(appSettings, savedApp, normalizeAppearance(savedApp));
  Object.assign(cloudSettings, normalizedCloud(loadSettings(CLOUD_SETTINGS_KEY, defaultCloudSettings)));
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
  Object.assign(cloudSettings, normalizedCloud(value));
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

export function shortcutFromKeyboardEvent(event: KeyboardEvent): string | null {
  if (["Control", "Shift", "Alt", "Meta"].includes(event.key) || !(event.ctrlKey || event.shiftKey || event.altKey || event.metaKey)) {
    return null;
  }
  const key = event.key.length === 1 ? event.key.toLocaleUpperCase() : event.key;
  return [
    event.ctrlKey && "Ctrl",
    event.shiftKey && "Shift",
    event.altKey && "Alt",
    event.metaKey && "Meta",
    key,
  ].filter(Boolean).join("+");
}
