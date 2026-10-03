import { describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  defaultBackupTrigger,
  validateBackupTrigger,
  acceptBackupRuntime,
  listBackupProcesses,
} from "./backupAutomation";

vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...await importOriginal<typeof import("@tauri-apps/api/core")>(),
  invoke: vi.fn(),
}));

describe("backup automation", () => {
  it("shows the newest running executable first and keeps its latest instance", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      processes: [
        { pid: 1, startTime: 10, name: "Z.exe", executablePath: "C:\\Z.exe" },
        { pid: 2, startTime: 30, name: "A.exe", executablePath: "C:\\A.exe" },
        { pid: 3, startTime: 20, name: "A.exe", executablePath: "C:\\A.exe" },
        { pid: 4, startTime: 40, name: "B.exe", executablePath: "C:\\B.exe" },
      ],
      complete: true,
      partial: false,
    });

    const result = await listBackupProcesses();
    expect(result.processes.map((process) => process.pid)).toEqual([4, 2, 1]);
  });
  it("defaults to the legacy file-change mode and requires an executable for exit mode", () => {
    expect(defaultBackupTrigger().mode).toBe("file_change");
    expect(
      validateBackupTrigger({ ...defaultBackupTrigger(), mode: "game_exit" }),
    ).toBe("backup_executable_required");
    expect(
      validateBackupTrigger({
        ...defaultBackupTrigger(),
        mode: "game_exit",
        executablePath: "C:\\game\\game.exe",
      }),
    ).toBeNull();
    expect(
      validateBackupTrigger({ ...defaultBackupTrigger(), quietSeconds: 301 }),
    ).toBe("backup_invalid_quiet_seconds");
  });
  it("ignores runtime events from an earlier configuration", () => {
    const current = {
      entryId: "a",
      generation: 2,
      status: "running",
      reasonCode: null,
    };
    expect(
      acceptBackupRuntime(current, {
        ...current,
        generation: 1,
        status: "waiting",
      }),
    ).toBe(current);
  });
});
