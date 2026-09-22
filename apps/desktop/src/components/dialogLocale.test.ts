import { afterEach, expect, it } from "vitest";
import { createSSRApp, h } from "vue";
import { renderToString } from "@vue/server-renderer";
import CreateCategoryDialog from "./CreateCategoryDialog.vue";
import TutorialThemePicker from "./TutorialThemePicker.vue";
import { setLocale } from "../services/i18n";

afterEach(() => setLocale("zh-CN"));

it("renders English category controls without translating a Chinese user category", async () => {
  setLocale("en");
  const html = await renderToString(createSSRApp({ render: () => h(CreateCategoryDialog, { parentName: "新建分类" }) }));
  expect(html).toContain("New subcategory");
  expect(html).toContain("Create in: 新建分类");
  expect(html).toContain("Category name");
  expect(html).toContain("Cancel");
});

it("keeps the default Chinese category controls", async () => {
  setLocale("zh-CN");
  const html = await renderToString(createSSRApp(CreateCategoryDialog));
  expect(html).toContain("新建分类");
  expect(html).toContain("分类名称");
});

it("renders English tutorial appearance choices and accessibility labels", async () => {
  setLocale("en");
  const html = await renderToString(createSSRApp({ render: () => h(TutorialThemePicker, { appearance: { colorMode: "dark", colorTheme: "teal" } }) }));
  expect(html).toContain("Choose Teal theme");
  expect(html).toContain("Teal theme, Dark interface preview");
  expect(html).toContain("Create snapshot");
});
