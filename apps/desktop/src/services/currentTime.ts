export function formatCurrentTime(value: Date): string {
  return `${value.getFullYear()}年${String(value.getMonth() + 1).padStart(2, "0")}月${String(value.getDate()).padStart(2, "0")}日 ${String(value.getHours()).padStart(2, "0")}:${String(value.getMinutes()).padStart(2, "0")}`;
}

export function millisecondsUntilNextMinute(value: Date): number {
  return 60_000 - (value.getSeconds() * 1_000 + value.getMilliseconds());
}
