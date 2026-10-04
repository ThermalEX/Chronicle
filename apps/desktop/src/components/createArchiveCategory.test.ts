import { describe, expect, it } from "vitest";
import { createSSRApp } from "vue";
import { renderToString } from "@vue/server-renderer";
import CreateArchiveDialog from "./CreateArchiveDialog.vue";
import { setLocale } from "../services/i18n";

const categories = [{ id: "games", name: "游戏" }, { id: "novels", name: "视觉小说" }];
const sources = [{ id: "one", name: "save", path: "C:\\save", kind: "folder" as const }];

async function render(props: Record<string, unknown> = {}) {
  let state: any;
  let emitted: any;
  const component = { ...CreateArchiveDialog, setup(input: unknown, context: any) {
    state = (CreateArchiveDialog as any).setup(input, context);
    return state;
  } };
  const html = await renderToString(createSSRApp(component, {
    sources, categories, defaultCategoryId: "games", defaultInitialSnapshot: false,
    onSubmit: (value: unknown) => { emitted = value; }, ...props,
  }));
  return { state, html, submitted: () => emitted };
}

describe("create archive category", () => {
  it("defaults to the current category and submits a changed or root category", async () => {
    const { state, submitted } = await render();
    state.name.value = "Game";
    expect(state.categoryId.value).toBe("games");
    state.categoryId.value = "novels";
    state.submit();
    expect(submitted().categoryId).toBe("novels");
    state.categoryId.value = "";
    state.submit();
    expect(submitted().categoryId).toBeUndefined();
  });

  it("does not show a category picker while editing", async () => {
    const { html } = await render({ editName: "Existing" });
    expect(html).not.toContain("保存到分类");
  });

  it("reports a vanished category rather than silently using root", async () => {
    const { state, submitted } = await render({ categories: [] });
    state.name.value = "Game";
    state.submit();
    expect(submitted()).toBeUndefined();
    expect(state.categoryError.value).not.toBe("");
  });

  it("shows translated category controls", async () => {
    setLocale("en");
    const { html } = await render();
    expect(html).toContain("Save in category");
    setLocale("zh-CN");
  });
});
