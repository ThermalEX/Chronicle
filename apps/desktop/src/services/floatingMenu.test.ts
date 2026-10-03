import { describe, expect, it } from "vitest";
import { floatingMenuStyle, positionFloatingMenu } from "./floatingMenu";

describe("positionFloatingMenu", () => {
  it("converts numeric geometry into browser-valid pixel values", () => {
    expect(floatingMenuStyle({ top: 95, left: 32, minWidth: 148 })).toEqual({
      top: "95px",
      left: "32px",
      minWidth: "148px",
    });
  });

  it("opens upward when a menu below its trigger would leave the viewport", () => {
    expect(positionFloatingMenu(
      { top: 40, right: 180, bottom: 74, left: 32, width: 148, height: 34 },
      { width: 160, height: 108 },
      { width: 300, height: 120 },
    )).toEqual({ top: 8, left: 32, minWidth: 148, maxHeight: 104 });
  });

  it("keeps the popup within the visible viewport rather than a scrolling parent", () => {
    expect(positionFloatingMenu(
      { top: 56, right: 180, bottom: 90, left: 32, width: 148, height: 34 },
      { width: 160, height: 72 },
      { width: 300, height: 260 },
    )).toEqual({ top: 95, left: 32, minWidth: 148 });
  });

  it("limits a long menu to the viewport so its last options remain reachable", () => {
    const position = positionFloatingMenu(
      { top: 150, right: 180, bottom: 184, left: 32, width: 148, height: 34 },
      { width: 160, height: 960 },
      { width: 300, height: 200 },
    );
    expect(position).toEqual({ top: 8, left: 32, minWidth: 148, maxHeight: 184 });
    expect(floatingMenuStyle(position)).toEqual({
      top: "8px",
      left: "32px",
      minWidth: "148px",
      maxHeight: "184px",
    });
  });
});
