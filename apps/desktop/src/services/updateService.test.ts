import { describe, expect, it } from "vitest";
import { releasesFromAtom, selectUpdate, type GithubRelease } from "./updateService";

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

describe("releasesFromAtom", () => {
  it("reads release metadata and derives the published installer links", () => {
    const feed = `<?xml version="1.0"?><feed><entry>
      <title>Chronicle v1.2.1-beta.1</title>
      <link rel="alternate" type="text/html" href="https://github.com/ThermalEX/Chronicle/releases/tag/v1.2.1-beta.1" />
      <updated>2026-09-10T00:00:00Z</updated>
      <content type="html">&lt;h2&gt;修复&lt;/h2&gt;&lt;ul&gt;&lt;li&gt;修复更新检查&lt;/li&gt;&lt;/ul&gt;</content>
    </entry></feed>`;

    expect(releasesFromAtom(feed)).toEqual([expect.objectContaining({
      tag_name: "v1.2.1-beta.1",
      prerelease: true,
      body: "## 修复\n\n- 修复更新检查",
      assets: expect.arrayContaining([
        expect.objectContaining({
          name: "Chronicle_1.2.1-beta.1_x64-setup.exe",
          browser_download_url: "https://github.com/ThermalEX/Chronicle/releases/download/v1.2.1-beta.1/Chronicle_1.2.1-beta.1_x64-setup.exe",
        }),
      ]),
    })]);
  });
});
