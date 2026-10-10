import { beforeEach, describe, expect, it, vi } from "vitest";
import { BrowserDeviceRepository, deviceLabel, deviceNameIndex } from "./devices";

describe("device identity", () => {
  beforeEach(() => {
    const items = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => items.get(key) ?? null,
      setItem: (key: string, value: string) => items.set(key, value),
    });
  });
  it("persists a trimmed name without changing ID", async () => {
    const repository = new BrowserDeviceRepository();
    const original = await repository.read();
    const renamed = await repository.rename("  Laptop  ");
    expect(renamed.id).toBe(original.id);
    expect(renamed.name).toBe("Laptop");
    expect(await new BrowserDeviceRepository().read()).toEqual(renamed);
  });
  it("validates Unicode length and preserves identity when invalid", async () => {
    const repository = new BrowserDeviceRepository();
    const original = await repository.read();
    await expect(repository.rename(" ")).rejects.toThrow();
    await expect(repository.rename("字".repeat(65))).rejects.toThrow();
    expect(await repository.read()).toEqual(original);
    expect((await repository.rename("字".repeat(64))).name.length).toBe(64);
  });
  it("creates a new ID only after an explicit reset", async () => {
    const repository = new BrowserDeviceRepository();
    const old = await repository.rename("Desktop");
    const next = await repository.reset();
    expect(next.id).not.toBe(old.id);
    expect(next.name).toBe(old.name);
  });
  it("uses known names then captured names, with short IDs for disambiguation", () => {
    expect(deviceLabel("12345678-abcd", "Old", [{ id: "12345678-abcd", name: "New", revision: 1 }])).toBe("New · 12345678");
    expect(deviceLabel("12345678-abcd", "Old", [])).toBe("Old · 12345678");
    expect(deviceLabel(undefined, undefined, [])).toBe("未知设备");
  });
  it("indexes names once for 5000 labels, preserving first-name priority and fallbacks", () => {
    let reads = 0;
    const known = Array.from({ length: 200 }, (_, i) => ({ get id() { reads++; return `device-${i}`; }, name: `Device ${i}`, revision: 1 }));
    known.unshift({ id: "device-199", name: "Local first", revision: 0 });
    const names = deviceNameIndex(known);
    reads = 0;
    for (let i = 0; i < 5000; i++) expect(deviceLabel("device-199", "Old", names)).toBe("Local first · device-1");
    expect(reads).toBe(0);
    expect(deviceLabel("12345678-abcd", "Captured", names)).toBe("Captured · 12345678");
    expect(deviceLabel("12345678-abcd", undefined, names)).toBe("12345678");
    expect(deviceLabel(undefined, undefined, names)).toBe("未知设备");
  });
});
