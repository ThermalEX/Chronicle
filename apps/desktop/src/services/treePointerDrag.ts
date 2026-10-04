export type TreeDragItem = { kind: "archive" | "category"; id: string };
type Point = { x: number; y: number };

export function createTreePointerDrag() {
  let pending: { item: TreeDragItem; point: Point } | undefined;
  let activeItem: TreeDragItem | undefined;
  let target: string | undefined;

  return {
    get activeItem() { return activeItem; },
    get target() { return target; },
    get isDragging() { return Boolean(activeItem); },
    start(item: TreeDragItem, point: Point) {
      pending = { item, point };
      activeItem = undefined;
      target = undefined;
    },
    move(point: Point, nextTarget?: string, targetAllowed = false) {
      if (pending && Math.hypot(point.x - pending.point.x, point.y - pending.point.y) > 5) {
        activeItem = pending.item;
        pending = undefined;
      }
      target = activeItem && targetAllowed ? nextTarget : undefined;
    },
    finish() {
      const result = activeItem ? target : undefined;
      pending = undefined;
      activeItem = undefined;
      target = undefined;
      return result;
    },
    cancel() {
      pending = undefined;
      activeItem = undefined;
      target = undefined;
    },
  };
}
