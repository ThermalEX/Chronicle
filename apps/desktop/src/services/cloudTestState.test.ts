import { describe, expect, it } from "vitest";
import { hasCloudSourceTestPassed, recordCloudSourceTest } from "./cloudTestState";
import type { CloudSource } from "./settings";

const source: CloudSource = {
  id: "r2", name: "R2", provider: "opendal", endpoint: "", username: "", remotePath: "/Chronicle", credentialRef: "r2",
  scheme: "s3", config: { bucket: "chronicle", region: "auto", endpoint: "https://example.r2.cloudflarestorage.com" }, secretKeys: ["access_key_id", "secret_access_key"], syncEnabled: true,
};

describe("cloud test state", () => {
  it("accepts the exact saved configuration that passed the global connection check", () => {
    recordCloudSourceTest(source);
    expect(hasCloudSourceTestPassed({ ...source, config: { ...source.config } })).toBe(true);
  });

  it("invalidates a passing result when the connection configuration changes", () => {
    recordCloudSourceTest(source);
    expect(hasCloudSourceTestPassed({ ...source, config: { ...source.config, bucket: "other" } })).toBe(false);
  });
});
