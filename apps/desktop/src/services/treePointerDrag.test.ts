import { describe, expect, it } from "vitest";
import { createTreePointerDrag } from "./treePointerDrag";

const archive = { kind: "archive" as const, id: "a" };
const category = { kind: "category" as const, id: "c" };

describe("tree pointer drag", () => {
  it("waits for movement beyond five pixels and preserves normal clicks", () => {
    const drag = createTreePointerDrag();
    drag.start(archive, { x: 10, y: 10 });
    drag.move({ x: 13, y: 14 }, "category:one", true);
    expect(drag.isDragging).toBe(false);
    expect(drag.finish()).toBeUndefined();
  });

  it("accepts allowed root, category and trash targets", () => {
    const drag = createTreePointerDrag();
    for (const target of ["category:all", "category:one", "trash"]) {
      drag.start(archive, { x: 0, y: 0 });
      drag.move({ x: 6, y: 0 }, target, true);
      expect(drag.isDragging).toBe(true);
      expect(drag.finish()).toBe(target);
    }
  });

  it("rejects a descendant target and cancels on Escape", () => {
    const drag = createTreePointerDrag();
    drag.start(category, { x: 0, y: 0 });
    drag.move({ x: 10, y: 0 }, "category:child", false);
    expect(drag.finish()).toBeUndefined();
    drag.start(category, { x: 0, y: 0 });
    drag.move({ x: 10, y: 0 }, "trash", true);
    drag.cancel();
    expect(drag.finish()).toBeUndefined();
  });
});
