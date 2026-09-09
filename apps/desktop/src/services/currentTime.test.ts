import { describe, expect, it } from "vitest";
import { formatCurrentTime, millisecondsUntilNextMinute } from "./currentTime";

describe("current time display", () => {
  it("pads hour and minute for the title bar", () => {
    expect(formatCurrentTime(new Date(2026, 8, 9, 7, 5))).toBe("07:05");
    expect(formatCurrentTime(new Date(2026, 8, 9, 18, 42))).toBe("18:42");
  });

  it("waits only until the next minute boundary before refreshing", () => {
    expect(millisecondsUntilNextMinute(new Date(2026, 8, 9, 7, 5, 0, 0))).toBe(60_000);
    expect(millisecondsUntilNextMinute(new Date(2026, 8, 9, 7, 5, 42, 250))).toBe(17_750);
  });
});
