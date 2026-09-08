import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

/** Opens an http(s) URL in the operating system's default browser. */
export async function openExternalUrl(url: string): Promise<void> {
  const parsed = new URL(url);
  if (parsed.protocol !== "https:" && parsed.protocol !== "http:") {
    throw new Error("Only HTTP(S) URLs can be opened externally.");
  }

  if (isTauri()) {
    await openUrl(parsed.href);
    return;
  }

  const opened = window.open(parsed.href, "_blank", "noopener,noreferrer");
  if (!opened) throw new Error("浏览器阻止了打开外部链接。");
}
