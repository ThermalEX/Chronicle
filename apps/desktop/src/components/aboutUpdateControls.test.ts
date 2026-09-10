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
  });

  it("opens the update dialog when a newer release is found", () => {
    expect(app).toContain("checkForApplicationUpdate");
    expect(app).toContain("<UpdateDialog");
    expect(app).toContain("@check-update");
  });
});
