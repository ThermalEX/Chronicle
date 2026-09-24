import { afterEach, expect, it } from "vitest";
import { computed } from "vue";
import { tutorialViews } from "./onboarding";
import { setLocale } from "./i18n";

afterEach(() => setLocale("zh-CN"));

it("translates tutorial text when read after a language change and preserves the Steam action", () => {
  const view = tutorialViews["steam-entry"]!;
  const title = computed(() => view.title);
  expect(title.value).toBe("游戏存档识别");
  setLocale("en");
  expect(title.value).toBe("Game save detection");
  expect(view.body).toContain("Opening the dialog does not start a scan.");
  expect(view.primaryLabel).toBe("Skip for now");
  expect(view.mode).toBe("action");
  expect(view.stage).toBe(7);
  setLocale("zh-CN");
  expect(title.value).toBe("游戏存档识别");
});
