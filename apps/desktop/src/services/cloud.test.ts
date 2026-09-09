import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => true }));

describe("cloud repository adapter", () => {
  beforeEach(() => invoke.mockReset());

  it("preserves the previous in-memory configuration if settings persistence fails", async () => {
    const { saveCloudSettings, cloudSettings } = await import("./settings");
    const previous = JSON.stringify(cloudSettings);
    invoke.mockRejectedValueOnce(new Error("settings failed"));
    await expect(saveCloudSettings({ ...cloudSettings, enabled: !cloudSettings.enabled, sources: [] })).rejects.toThrow("settings failed");
    expect(JSON.stringify(cloudSettings)).toBe(previous);
  });

  it("rejects unknown public config before writing settings", async () => {
    const { saveCloudSettings, cloudSettings } = await import("./settings");
    const source = { id: "dal-bad", name: "S3", provider: "opendal" as const, endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "dal-bad", scheme: "s3", config: { secret_access_key: "secret" }, secretKeys: [] };
    await expect(saveCloudSettings({ ...cloudSettings, sources: [source] })).rejects.toThrow("机密");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("does not save settings or enable a source when credential persistence fails", async () => {
    const { saveCloudConfiguration } = await import("./cloud");
    const { cloudSettings } = await import("./settings");
    const source = { id: "dal-1", name: "S3", provider: "opendal" as const, endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "dal-1", scheme: "s3", config: { bucket: "bucket" }, secretKeys: ["secret_access_key"] };
    invoke.mockRejectedValueOnce(new Error("credential failed"));
    await expect(saveCloudConfiguration({ ...cloudSettings, enabled: true, sources: [source] }, [{ source, secrets: { secret_access_key: "secret" } }])).rejects.toThrow("credential failed");
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke.mock.calls[0][0]).toBe("save_opendal_credential");
    expect(cloudSettings.enabled).toBe(false);
  });

  it("saves credentials before a v3 settings document with no secret values", async () => {
    const { saveCloudConfiguration } = await import("./cloud");
    const { cloudSettings } = await import("./settings");
    const source = { id: "dal-2", name: "S3", provider: "opendal" as const, endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "dal-2", scheme: "s3", config: { bucket: "bucket" }, secretKeys: ["secret_access_key"] };
    invoke.mockResolvedValue(undefined);
    await saveCloudConfiguration({ ...cloudSettings, enabled: true, sources: [source] }, [{ source, secrets: { secret_access_key: "never-persist-here" } }]);
    expect(invoke.mock.calls.map((call) => call[0])).toEqual(["save_opendal_credential", "save_settings"]);
    const settings = invoke.mock.calls[1][1].settings;
    expect(settings.formatVersion).toBe(3);
    expect(JSON.stringify(settings)).not.toContain("never-persist-here");
  });

  it("stores OpenDAL secret patches separately from settings after backend validation", async () => {
    const { cloudRepository } = await import("./cloud");
    const source = { id: "dal-1", name: "S3", provider: "opendal" as const, endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "dal-1", scheme: "s3", config: { bucket: "bucket" }, secretKeys: ["secret_access_key"] };
    await cloudRepository.test(source, JSON.stringify({ secret_access_key: "secret" }));
    await cloudRepository.saveOpenDalCredential(source, { secret_access_key: "secret" });
    expect(invoke).toHaveBeenNthCalledWith(1, "test_cloud_source", { source, password: '{"secret_access_key":"secret"}' });
    expect(invoke).toHaveBeenNthCalledWith(2, "save_opendal_credential", { source, secrets: { secret_access_key: "secret" } });
    expect(JSON.stringify(source)).not.toContain(':"secret"');
  });

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
      id: "github-1", name: "GitHub", provider: "legacy_github", endpoint: "", username: "",
      remotePath: "/Chronicle", credentialRef: "chronicle-github:github-1", repository: "owner/repository", branch: "main",
    }, "github_pat_secret");
    expect(invoke).toHaveBeenCalledWith("save_cloud_credential", {
      sourceId: "github-1", credentialRef: "chronicle-github:github-1", password: "github_pat_secret", provider: "legacy_github",
    });
  });

  it("loads only credential-presence flags for configured sources", async () => {
    const { cloudRepository } = await import("./cloud");
    invoke.mockResolvedValueOnce([{ sourceId: "github-1", credentialSaved: true }]);
    await expect(cloudRepository.sourceStatuses()).resolves.toEqual([{ sourceId: "github-1", credentialSaved: true }]);
    expect(invoke).toHaveBeenCalledWith("cloud_source_statuses");
  });

  it("asks the desktop backend to create a private GitHub repository", async () => {
    const { cloudRepository } = await import("./cloud");
    const source = {
      id: "github-1", name: "GitHub", provider: "legacy_github" as const, endpoint: "", username: "",
      remotePath: "/Chronicle", credentialRef: "chronicle-github:github-1", repository: "", branch: "main",
    };
    await cloudRepository.createGitHubRepository(source, "chronicle", "github_pat_secret");
    expect(invoke).toHaveBeenCalledWith("create_github_repository", {
      source, repositoryName: "chronicle", password: "github_pat_secret",
    });
  });
});
