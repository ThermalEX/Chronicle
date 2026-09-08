import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => true }));

describe("cloud repository adapter", () => {
  beforeEach(() => invoke.mockReset());

  it("uses explicit destructive and non-destructive WebDAV commands", async () => {
    invoke.mockResolvedValue({ status: "current", message: "already current" });
    const { cloudRepository } = await import("./cloud");
    await cloudRepository.sync("source-1", "entry-1");
    expect(invoke).toHaveBeenCalledWith("cloud_sync_entry", { sourceId: "source-1", entryId: "entry-1" });
    await cloudRepository.upload("source-1", "entry-1");
    expect(invoke).toHaveBeenCalledWith("cloud_overwrite_upload", { sourceId: "source-1", entryId: "entry-1" });
    await cloudRepository.download("source-1", "entry-1");
    expect(invoke).toHaveBeenCalledWith("cloud_overwrite_download", { sourceId: "source-1", entryId: "entry-1" });
    await cloudRepository.delete("source-1", ["entry-1", "entry-2"]);
    expect(invoke).toHaveBeenCalledWith("cloud_delete_entries", { sourceId: "source-1", entryIds: ["entry-1", "entry-2"] });
  });

  it("stores a GitHub token through the provider-specific credential command", async () => {
    const { cloudRepository } = await import("./cloud");
    await cloudRepository.saveCredential({
      id: "github-1", name: "GitHub", provider: "github", endpoint: "", username: "",
      remotePath: "/Chronicle", credentialRef: "chronicle-github:github-1", repository: "owner/repository", branch: "main",
    }, "github_pat_secret");
    expect(invoke).toHaveBeenCalledWith("save_cloud_credential", {
      sourceId: "github-1", credentialRef: "chronicle-github:github-1", password: "github_pat_secret", provider: "github",
    });
  });
});
