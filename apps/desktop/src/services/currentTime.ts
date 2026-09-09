export function formatCurrentTime(value: Date): string {
  return `${String(value.getHours()).padStart(2, "0")}:${String(value.getMinutes()).padStart(2, "0")}`;
}

export function millisecondsUntilNextMinute(value: Date): number {
  return 60_000 - (value.getSeconds() * 1_000 + value.getMilliseconds());
}
