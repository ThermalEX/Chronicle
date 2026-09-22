import { locale } from "./i18n";

export function formatCurrentTime(value: Date, language = locale.value): string {
  if (language === "en") return value.toLocaleString("en-US", { year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: false });
  return `${value.getFullYear()}年${String(value.getMonth() + 1).padStart(2, "0")}月${String(value.getDate()).padStart(2, "0")}日 ${String(value.getHours()).padStart(2, "0")}:${String(value.getMinutes()).padStart(2, "0")}`;
}

export function millisecondsUntilNextMinute(value: Date): number {
  return 60_000 - (value.getSeconds() * 1_000 + value.getMilliseconds());
}
