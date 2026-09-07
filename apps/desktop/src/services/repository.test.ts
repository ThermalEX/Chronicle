import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const open = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => true }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));

describe("desktop repository adapter", () => {
  beforeEach(() => {
    invoke.mockReset();
    open.mockReset();
  });

  it("uses the native dialog and Rust command in a Tauri runtime", async () => {
    open.mockResolvedValue("C:\\Users\\ThermalEX\\Documents\\Example");
    invoke.mockResolvedValue({ id: "entry-1", name: "Example" });
    const { archiveRepository, isTauriRuntime } = await import("./repository");

    const entry = await archiveRepository.addArchive("folder", "工作");

    expect(isTauriRuntime).toBe(true);
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ directory: true, multiple: false }));
    expect(invoke).toHaveBeenCalledWith("add_entry", {
      sourcePath: "C:\\Users\\ThermalEX\\Documents\\Example",
      categoryId: "工作",
    });
    expect(entry).toEqual({ id: "entry-1", name: "Example" });
  });
});
