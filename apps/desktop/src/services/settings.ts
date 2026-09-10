import { invoke, isTauri } from "@tauri-apps/api/core";
import { reactive } from "vue";
import { normalizeAppearance, type ColorMode, type ColorTheme } from "./appearance";
import { publicConfigKeys } from "./opendal";

export type CloseBehavior = "ask" | "tray" | "exit";
export type BackupSchedule = "off" | "15m" | "1h" | "6h" | "daily";
export type CloudProvider = "legacy_webdav" | "legacy_github" | "opendal";

export interface AppSettings {
  colorTheme: ColorTheme;
  colorMode: ColorMode;
  launchAtStartup: boolean;
  closeBehavior: CloseBehavior;
  checkForUpdates: boolean;
  checkCloudOnLaunch: boolean;
  notifications: boolean;
  createInitialSnapshot: boolean;
  autoBackupDelaySeconds: number;
  retentionCount: number | null;
  recycleBinEnabled: boolean;
  recycleBinPath: string;
  searchShortcut: string;
  snapshotShortcut: string;
  settingsShortcut: string;
}

export type CloudHealthStatus = "unchecked" | "checking" | "available" | "unavailable";

export interface CloudHealth {
  status: CloudHealthStatus;
  sourceName?: string;
  reason?: string;
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
  scheme?: string;
  config?: Record<string, string>;
  secretKeys?: string[];
  syncEnabled?: boolean;
}

export interface CloudSettings {
  enabled: boolean;
  sources: CloudSource[];
  maxConcurrentMetadataReads: number;
  maxConcurrentTransfers: number;
  requestDelayMs: number;
  retryLimit: number;
}

export function cloudLibraryIndicator(
  settings: Pick<CloudSettings, "enabled" | "sources">,
  health: CloudHealth = { status: "unchecked" },
): { available: boolean; label: string } {
  if (!settings.enabled || !settings.sources.length) return { available: false, label: "云端资料库未启用" };
  if (health.status === "checking") return { available: false, label: "云端资料库：检测中" };
  if (health.status === "unavailable") return { available: false, label: `云端资料库：${health.sourceName ?? "同步源"} 无法使用` };
  if (health.status === "available") return { available: true, label: `云端资料库：${settings.sources.length} 个同步源可用` };
  return { available: false, label: "云端资料库：未检测" };
}

const APP_SETTINGS_KEY = "chronicle.app-settings.v2";
const CLOUD_SETTINGS_KEY = "chronicle.cloud-settings.v2";

export interface SettingsDocument {
  formatVersion: 3;
  app: AppSettings;
  cloud: CloudSettings;
}

const defaultAppSettings: AppSettings = {
  colorTheme: "teal",
  colorMode: "light",
  launchAtStartup: false,
  closeBehavior: "ask",
  checkForUpdates: true,
  checkCloudOnLaunch: false,
  notifications: true,
  createInitialSnapshot: true,
  autoBackupDelaySeconds: 5,
  retentionCount: null,
  recycleBinEnabled: true,
  recycleBinPath: "",
  searchShortcut: "Ctrl+K",
  snapshotShortcut: "Ctrl+Shift+B",
  settingsShortcut: "Ctrl+,",
};

const defaultCloudSettings: CloudSettings = {
  enabled: false,
  sources: [],
  maxConcurrentMetadataReads: 2,
  maxConcurrentTransfers: 2,
  requestDelayMs: 150,
  retryLimit: 5,
};

type LegacyCloudSettings = Partial<CloudSettings> & {
  activeSourceId?: string | null;
  endpoint?: string;
  username?: string;
  remotePath?: string;
};

type LegacyAppSettings = Partial<AppSettings> & { backupSchedule?: BackupSchedule; autoBackupEnabled?: boolean; automaticUploadEnabled?: boolean };

export function normalizeAppSettings(value: LegacyAppSettings = {}): AppSettings {
  const legacyDelay: Record<Exclude<BackupSchedule, "off">, number> = { "15m": 900, "1h": 3600, "6h": 21_600, daily: 86_400 };
  const legacySchedule = value.backupSchedule;
  const delay = Number(value.autoBackupDelaySeconds ?? (legacySchedule && legacySchedule !== "off" ? legacyDelay[legacySchedule] : 5));
  const { backupSchedule: _backupSchedule, autoBackupEnabled: _autoBackupEnabled, automaticUploadEnabled: _automaticUploadEnabled, ...current } = value;
  return {
    ...defaultAppSettings,
    ...current,
    autoBackupDelaySeconds: Math.max(1, Math.min(300, Number.isFinite(delay) && delay >= 1 ? delay : 5)),
  };
}

export function normalizedCloud(value?: LegacyCloudSettings): CloudSettings {
  const raw = value ?? {};
  const legacyActiveSourceId = raw.activeSourceId ?? null;
  let sources = Array.isArray(raw.sources) ? raw.sources.map((source) => {
    const provider = String(source.provider);
    return {
      ...source,
      provider: (provider === "github" ? "legacy_github" : provider === "webdav" ? "legacy_webdav" : provider) as CloudProvider,
      ...(source.config ? { config: { ...source.config } } : {}),
      ...(source.secretKeys ? { secretKeys: [...source.secretKeys] } : {}),
      syncEnabled: typeof source.syncEnabled === "boolean" ? source.syncEnabled : source.id === legacyActiveSourceId,
    };
  }) : [];
  if (!sources.length && raw.endpoint) {
    const id = crypto.randomUUID();
    sources = [{
      id,
      name: "WebDAV",
      provider: "legacy_webdav",
      endpoint: raw.endpoint,
      username: raw.username ?? "",
      remotePath: raw.remotePath || "/Chronicle",
      credentialRef: `chronicle-webdav:${id}`,
      syncEnabled: true,
    }];
  }
  const requestedDelay = Number(raw.requestDelayMs ?? 150);
  return {
    enabled: Boolean(raw.enabled && sources.length),
    sources,
    maxConcurrentMetadataReads: Math.max(1, Math.min(4, Number(raw.maxConcurrentMetadataReads) || 2)),
    maxConcurrentTransfers: Math.max(1, Math.min(4, Number(raw.maxConcurrentTransfers) || 2)),
    requestDelayMs: Math.max(0, Math.min(5000, Number.isFinite(requestedDelay) ? requestedDelay : 150)),
    retryLimit: Math.max(1, Math.min(10, Number(raw.retryLimit) || 5)),
  };
}

export function enabledCloudSources(settings: Pick<CloudSettings, "enabled" | "sources">): CloudSource[] {
  return settings.enabled ? settings.sources.filter((source) => source.syncEnabled) : [];
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
    formatVersion: 3,
    app: { ...appSettings },
    cloud: { ...cloudSettings, sources: cloudSettings.sources.map((source) => ({ ...source })) },
  };
}

export async function initializeSettings(): Promise<void> {
  if (isTauri()) {
    const saved = await invoke<Partial<SettingsDocument> & { cloud?: LegacyCloudSettings }>("load_settings");
    Object.assign(appSettings, normalizeAppSettings(saved.app), normalizeAppearance(saved.app ?? {}));
    Object.assign(cloudSettings, normalizedCloud(saved.cloud));
    if (saved.formatVersion !== 3) await persistSettings();
    return;
  }
  const savedApp = loadSettings(APP_SETTINGS_KEY, defaultAppSettings);
  Object.assign(appSettings, normalizeAppSettings(savedApp), normalizeAppearance(savedApp));
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
  const normalized = normalizedCloud(value);
  for (const source of normalized.sources) {
    if (source.provider === "opendal" && Object.keys(source.config ?? {}).some((key) => !publicConfigKeys.includes(key) || source.secretKeys?.includes(key))) {
      throw new Error("机密值不能保存在公开设置中");
    }
  }
  if (isTauri()) {
    await invoke("save_settings", { settings: { formatVersion: 3, app: { ...appSettings }, cloud: normalized } });
  } else {
    localStorage.setItem(CLOUD_SETTINGS_KEY, JSON.stringify(normalized));
  }
  Object.assign(cloudSettings, normalized);
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
