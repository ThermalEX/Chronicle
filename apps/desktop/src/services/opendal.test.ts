import { describe, expect, it } from "vitest";
import { configureOpenDal, openDalTemplates, secretPatch, sourceTestKey, validateAdvancedKey, validateOpenDal } from "./opendal";
import { normalizedCloud, type CloudSource } from "./settings";

function source(): CloudSource {
  return { id: "source", name: "Cloud", provider: "opendal", endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "credential:old" };
}

describe("OpenDAL configuration", () => {
  it("preserves a zero request delay during migration and clamps invalid values", () => {
    expect(normalizedCloud({ requestDelayMs: 0 }).requestDelayMs).toBe(0);
    expect(normalizedCloud({ requestDelayMs: Number.NaN }).requestDelayMs).toBe(150);
    expect(normalizedCloud({ requestDelayMs: -10 }).requestDelayMs).toBe(0);
    expect(normalizedCloud({ requestDelayMs: 9000 }).requestDelayMs).toBe(5000);
  });
  it("rejects credentials and non-HTTP URLs in the public endpoint", () => {
    const value = source(); configureOpenDal(value, "s3"); value.config!.bucket = "bucket";
    for (const endpoint of ["https://user:password@example.com", "https://example.com?token=secret", "https://example.com#secret", "file:///secret", "ftp://example.com", "invalid-url"]) {
      value.config!.endpoint = endpoint;
      expect(validateOpenDal(value)).toContain("endpoint");
      expect(validateOpenDal(value)).not.toContain("password");
    }
    value.config!.endpoint = "https://example.com/storage";
    expect(validateOpenDal(value)).toBe("");
  });
  it("offers exactly the compiled services and keeps tokens out of config", () => {
    expect(openDalTemplates.map((item) => item.scheme).sort()).toEqual(["azblob", "b2", "cos", "gcs", "github", "obs", "oss", "s3", "swift", "tos", "webdav"]);
    const value = source();
    configureOpenDal(value, "s3");
    expect(value.config).toEqual({ bucket: "", region: "", endpoint: "" });
    expect(value.secretKeys).toEqual(["access_key_id", "secret_access_key"]);
    expect(() => configureOpenDal(value, "fs")).toThrow("未编译");
  });
  it("invalidates a successful test on config, root or credential changes", () => {
    const value = source(); configureOpenDal(value, "s3");
    const key = sourceTestKey(value, { secret_access_key: "old" });
    expect(sourceTestKey(value, { secret_access_key: "new" })).not.toBe(key);
    expect(sourceTestKey({ ...value, remotePath: "/other" }, { secret_access_key: "old" })).not.toBe(key);
    expect(sourceTestKey({ ...value, config: { bucket: "other" } }, { secret_access_key: "old" })).not.toBe(key);
  });
  it("keeps blank secrets as an unchanged credential patch and rejects unsafe advanced keys", () => {
    expect(secretPatch({ token: "", session_token: "new" })).toEqual({ session_token: "new" });
    for (const key of ["root", "branch", "credential_path", "credential_file", "google_application_credentials", "aws_profile", "AccessKey", "../key"]) expect(validateAdvancedKey(key)).not.toBe("");
    expect(validateAdvancedKey("session_token")).toBe("");
  });
  it("rejects secret leakage and reports missing fields without values", () => {
    const value = source(); configureOpenDal(value, "s3");
    expect(validateOpenDal(value)).toBe("请填写 bucket");
    value.config = { bucket: "bucket", secret_access_key: "never-display" };
    expect(validateOpenDal(value)).not.toContain("never-display");
    expect(validateOpenDal(value)).toContain("机密");
  });
  it("migrates v2 source identity, branch and every path without mutation", () => {
    const legacy = { ...source(), provider: "github", repository: "owner/repo", branch: "custom/branch", remotePath: "/existing/path" };
    const migrated = normalizedCloud({ enabled: true, activeSourceId: legacy.id, sources: [legacy] as unknown as CloudSource[] });
    expect(migrated.sources[0]).toEqual({ ...legacy, provider: "legacy_github" });
    expect(legacy.provider).toBe("github");
    expect(normalizedCloud({ sources: [{ ...legacy, provider: "webdav" }] as unknown as CloudSource[] }).sources[0].provider).toBe("legacy_webdav");
  });
  it("clones OpenDAL nested fields so cancelled drafts cannot modify saved config", () => {
    const value = source(); configureOpenDal(value, "s3");
    const migrated = normalizedCloud({ sources: [value] });
    migrated.sources[0].config!.bucket = "new";
    migrated.sources[0].secretKeys!.push("session_token");
    expect(value.config!.bucket).toBe("");
    expect(value.secretKeys).not.toContain("session_token");
  });
});
