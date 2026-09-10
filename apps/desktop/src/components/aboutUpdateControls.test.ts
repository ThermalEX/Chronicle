import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const settingsDialog = readFileSync(new URL("./SettingsDialog.vue", import.meta.url), "utf8");
const app = readFileSync(new URL("../App.vue", import.meta.url), "utf8");

describe("About update controls", () => {
  it("keeps update channel, launch checks, and manual checks in About", () => {
    const aboutSection = settingsDialog.slice(settingsDialog.indexOf('<section v-else aria-labelledby="about-title">'));

    expect(aboutSection).toContain("更新频道");
    expect(aboutSection).toContain("启动时检查更新");
    expect(aboutSection).toContain("检查更新");
    expect(aboutSection).toContain("emit('check-update'");
    expect(aboutSection.indexOf('class="setting-group about-update-group"')).toBeGreaterThan(
      aboutSection.indexOf('class="about-list"'),
    );
    expect(aboutSection.indexOf('class="setting-group about-update-group"')).toBeGreaterThan(
      aboutSection.indexOf("查看 GitHub 仓库"),
    );
  });

  it("opens the update dialog when a newer release is found", () => {
    expect(app).toContain("checkForApplicationUpdate");
    expect(app).toContain("<UpdateDialog");
    expect(app).toContain("@check-update");
  });
});
