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
  backupSchedule: BackupSchedule;
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

export function cloudLibraryIndicator(
  settings: Pick<CloudSettings, "enabled" | "activeSourceId" | "sources">,
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

export function normalizedCloud(value?: LegacyCloudSettings): CloudSettings {
  const raw = value ?? {};
  let sources = Array.isArray(raw.sources) ? raw.sources.map((source) => {
    const provider = String(source.provider);
    return {
      ...source,
      provider: (provider === "github" ? "legacy_github" : provider === "webdav" ? "legacy_webdav" : provider) as CloudProvider,
      ...(source.config ? { config: { ...source.config } } : {}),
      ...(source.secretKeys ? { secretKeys: [...source.secretKeys] } : {}),
    };
  }) : [];
  let activeSourceId = raw.activeSourceId ?? null;
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
    }];
    activeSourceId = id;
  }
  if (!sources.some((source) => source.id === activeSourceId)) activeSourceId = sources[0]?.id ?? null;
  const requestedDelay = Number(raw.requestDelayMs ?? 150);
  return {
    enabled: Boolean(raw.enabled && sources.length),
    activeSourceId,
    sources,
    maxConcurrentMetadataReads: Math.max(1, Math.min(4, Number(raw.maxConcurrentMetadataReads) || 2)),
    maxConcurrentTransfers: Math.max(1, Math.min(4, Number(raw.maxConcurrentTransfers) || 2)),
    requestDelayMs: Math.max(0, Math.min(5000, Number.isFinite(requestedDelay) ? requestedDelay : 150)),
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
    formatVersion: 3,
    app: { ...appSettings },
    cloud: { ...cloudSettings, sources: cloudSettings.sources.map((source) => ({ ...source })) },
  };
}

export async function initializeSettings(): Promise<void> {
  if (isTauri()) {
    const saved = await invoke<Partial<SettingsDocument> & { cloud?: LegacyCloudSettings }>("load_settings");
    Object.assign(appSettings, defaultAppSettings, saved.app ?? {}, normalizeAppearance(saved.app ?? {}));
    Object.assign(cloudSettings, normalizedCloud(saved.cloud));
    if (saved.formatVersion !== 3) await persistSettings();
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
