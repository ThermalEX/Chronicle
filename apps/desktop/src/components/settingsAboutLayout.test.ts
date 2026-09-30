import { describe, expect, it } from "vitest";
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
import SettingsDialog from "./SettingsDialog.vue";

describe("settings About layout", () => {
  it("shows the tutorial action inside the app card without a separate tutorial row", async () => {
    const dialog = {
      ...SettingsDialog,
      setup(props: unknown, context: unknown) {
        const state = (SettingsDialog as any).setup(props, context);
        state.activeSection.value = "about";
        return state;
      },
    };
    const html = await renderToString(createSSRApp(dialog));
    const cardStart = html.indexOf('class="about-card"');
    const detailsStart = html.indexOf('class="about-list"');

    expect(html).toMatch(/<p[^>]*>SETTING<\/p><h2 id="settings-title"[^>]*>设置<\/h2>/);
    expect(cardStart).toBeGreaterThan(-1);
    expect(detailsStart).toBeGreaterThan(cardStart);
    expect(html.slice(cardStart, detailsStart)).toContain("重新开始教程");
    expect(html.slice(detailsStart)).not.toContain("新手教程</b>");
  });
});
