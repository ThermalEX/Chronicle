import { describe, expect, it } from "vitest";
import { positionFloatingMenu } from "./floatingMenu";

describe("positionFloatingMenu", () => {
  it("opens upward when a menu below its trigger would leave the viewport", () => {
    expect(positionFloatingMenu(
      { top: 40, right: 180, bottom: 74, left: 32, width: 148, height: 34 },
      { width: 160, height: 108 },
      { width: 300, height: 120 },
    )).toEqual({ top: 8, left: 32, minWidth: 148 });
  });

  it("keeps the popup within the visible viewport rather than a scrolling parent", () => {
    expect(positionFloatingMenu(
      { top: 56, right: 180, bottom: 90, left: 32, width: 148, height: 34 },
      { width: 160, height: 72 },
      { width: 300, height: 260 },
    )).toEqual({ top: 95, left: 32, minWidth: 148 });
  });
});
