import { describe, expect, it } from "vitest";
import { selectUpdate, type GithubRelease } from "./updateService";

const releases: GithubRelease[] = [
  {
    tag_name: "v1.2.1-beta.1",
    name: "Chronicle v1.2.1-beta.1",
    body: "测试版说明",
    html_url: "https://github.com/ThermalEX/Chronicle/releases/tag/v1.2.1-beta.1",
    prerelease: true,
    draft: false,
    published_at: "2026-09-10T00:00:00Z",
    assets: [],
  },
  {
    tag_name: "v1.2.0",
    name: "Chronicle v1.2.0",
    body: "正式版说明",
    html_url: "https://github.com/ThermalEX/Chronicle/releases/tag/v1.2.0",
    prerelease: false,
    draft: false,
    published_at: "2026-09-09T00:00:00Z",
    assets: [],
  },
];

describe("selectUpdate", () => {
  it("ignores prereleases on the stable channel", () => {
    expect(selectUpdate(releases, "1.1.0", "stable")?.version).toBe("1.2.0");
  });

  it("uses the newest prerelease on the beta channel", () => {
    expect(selectUpdate(releases, "1.1.0", "beta")?.version).toBe("1.2.1-beta.1");
  });

  it("does not return an update for an equal version", () => {
    expect(selectUpdate([releases[1]], "1.2.0", "stable")).toBeUndefined();
  });
});
