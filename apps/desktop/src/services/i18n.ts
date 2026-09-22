import { ref, watch } from "vue";
import { commonMessages } from "../locales/common";
import { settingsMessages } from "../locales/settings";
import { dialogMessages } from "../locales/dialogs";
import { workspaceMessages } from "../locales/workspace";

export type Locale = "zh-CN" | "en";
export const locale = ref<Locale>("zh-CN");
export function normalizeLocale(value: unknown): Locale {
  return value === "en" ? "en" : "zh-CN";
}
export function setLocale(value: unknown): void {
  locale.value = normalizeLocale(value);
}

const messages: Record<string, string> = { ...commonMessages, ...settingsMessages, ...dialogMessages, ...workspaceMessages };

/** Translate only application-owned messages, never arbitrary user data or remote errors. */
export function t(source: string, params: Record<string, string | number> = {}): string {
  const message = locale.value === "en" ? messages[source] ?? source : source;
  return message.replace(/\{(\w+)\}/g, (token, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : token);
}

watch(locale, (value) => {
  if (typeof document !== "undefined") document.documentElement.lang = value;
}, { immediate: true, flush: "sync" });
