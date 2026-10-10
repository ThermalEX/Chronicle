import { invoke, isTauri } from "@tauri-apps/api/core";
import { t } from "./i18n";

export interface DeviceIdentity { id: string; name: string; revision: number }
export interface KnownSourceDevices { sourceId: string; sourceName: string; devices: (DeviceIdentity & { lastSuccessfulSyncAt: number | null })[] }
export function readKnownDevices(): Promise<KnownSourceDevices[]> { return isTauri() ? invoke("read_known_devices") : Promise.resolve([]); }
const key = "chronicle.device.v1";

export class BrowserDeviceRepository {
  async read(): Promise<DeviceIdentity> {
    const stored = localStorage.getItem(key);
    if (stored) return JSON.parse(stored) as DeviceIdentity;
    const device = { id: crypto.randomUUID(), name: "浏览器设备", revision: 0 };
    localStorage.setItem(key, JSON.stringify(device));
    return device;
  }
  async rename(name: string): Promise<DeviceIdentity> {
    name = name.trim();
    if (!name || [...name].length > 64) throw new Error(t("设备名称须为 1–64 个字符"));
    const device = await this.read();
    if (device.name !== name) { device.name = name; device.revision += 1; }
    localStorage.setItem(key, JSON.stringify(device));
    return device;
  }
  async reset(): Promise<DeviceIdentity> {
    const device = { ...await this.read(), id: crypto.randomUUID(), revision: 0 };
    localStorage.setItem(key, JSON.stringify(device));
    return device;
  }
}

export function deviceNameIndex(known: readonly DeviceIdentity[]): ReadonlyMap<string, string> {
  const names = new Map<string, string>();
  for (const device of known) if (!names.has(device.id)) names.set(device.id, device.name);
  return names;
}

export function deviceLabel(id: string | undefined, capturedName: string | undefined, known: DeviceIdentity[] | ReadonlyMap<string, string>): string {
  if (!id) return t("未知设备");
  const name = (Array.isArray(known) ? known.find((device) => device.id === id)?.name : known.get(id)) || capturedName;
  return name ? `${name} · ${id.slice(0, 8)}` : id.slice(0, 8);
}

export const deviceRepository = isTauri() ? {
  read: () => invoke<DeviceIdentity>("read_device"),
  rename: (name: string) => invoke<DeviceIdentity>("rename_device", { name }),
  reset: () => invoke<DeviceIdentity>("reset_device_identity"),
} : new BrowserDeviceRepository();
