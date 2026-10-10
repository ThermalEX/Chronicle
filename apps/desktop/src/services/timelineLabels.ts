import { t } from "./i18n";

export function createTimelineLabels() {
  let currentLocale = "", currentDay = "";
  let time: Intl.DateTimeFormat, dateTime: Intl.DateTimeFormat;
  const labels = new Map<number, string>();
  return (timestamp: number | undefined, language: string, now: Date): string => {
    if (!timestamp) return t("尚未备份");
    const day = now.toDateString();
    if (currentLocale !== language) {
      currentLocale = language;
      time = new Intl.DateTimeFormat(language, { hour: "2-digit", minute: "2-digit", hour12: false });
      dateTime = new Intl.DateTimeFormat(language, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: false });
      labels.clear();
    }
    if (currentDay !== day) { currentDay = day; labels.clear(); }
    const cached = labels.get(timestamp);
    if (cached !== undefined) return cached;
    const date = new Date(timestamp);
    const label = date.toDateString() === day ? t("今天 {time}", { time: time.format(date) }) : dateTime.format(date);
    // Keep only a bounded working set when the app remains open for many days.
    if (labels.size >= 4096) labels.clear();
    labels.set(timestamp, label);
    return label;
  };
}
