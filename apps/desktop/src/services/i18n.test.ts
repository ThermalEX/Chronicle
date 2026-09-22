import { afterEach, describe, expect, it } from "vitest";
import { computed } from "vue";
import { locale, normalizeLocale, setLocale, t } from "./i18n";

afterEach(() => setLocale("zh-CN"));

describe("application language", () => {
  it("defaults unsupported and legacy settings to Chinese", () => {
    expect(normalizeLocale(undefined)).toBe("zh-CN");
    expect(normalizeLocale("fr")).toBe("zh-CN");
    expect(normalizeLocale("en")).toBe("en");
  });
  it("updates existing computed labels immediately", () => {
    const label = computed(() => t("取消"));
    expect(label.value).toBe("取消");
    setLocale("en");
    expect(locale.value).toBe("en");
    expect(label.value).toBe("Cancel");
    setLocale("zh-CN");
    expect(label.value).toBe("取消");
  });
  it("interpolates named values without translating or reprocessing user content", () => {
    setLocale("en");
    expect(t("共 {count} 项", { count: 3 })).toBe("3 items");
    expect(t("Missing {name}", { name: "中文存档 {count} $&" })).toBe("Missing 中文存档 {count} $&");
    expect(t("unknown message")).toBe("unknown message");
    setLocale("zh-CN");
    expect(t("共 {count} 项", { count: 3 })).toBe("共 3 项");
  });
});
