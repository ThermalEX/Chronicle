import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => true }));

describe("diagnostics repository", () => {
  beforeEach(() => invoke.mockReset());

  it("persists redacted error records through the desktop command", async () => {
    const { diagnosticsRepository } = await import("./diagnostics");
    await diagnosticsRepository.record(new Error("Authorization: Bearer ghp_secret"), { operation: "同步存档", archiveId: "entry-1" });

    expect(invoke).toHaveBeenCalledWith("append_diagnostic", expect.objectContaining({
      entry: expect.objectContaining({ operation: "同步存档", archiveId: "entry-1" }),
    }));
    const call = invoke.mock.calls[0]?.[1] as { entry: { details: string } };
    expect(call.entry.details).not.toContain("ghp_secret");
  });
});
