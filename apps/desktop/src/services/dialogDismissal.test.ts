import { describe, expect, it, vi } from "vitest";

import { createBackdropDismissal } from "./dialogDismissal";

function pointer(pointerId: number, target: EventTarget, currentTarget: EventTarget): PointerEvent {
  return { pointerId, target, currentTarget } as PointerEvent;
}

describe("backdrop dismissal", () => {
  it("closes only when the pointer starts and ends on the backdrop", () => {
    const dismiss = vi.fn();
    const backdrop = new EventTarget();
    const dialog = new EventTarget();
    const controller = createBackdropDismissal(dismiss);

    controller.pointerDown(pointer(1, backdrop, backdrop));
    controller.pointerUp(pointer(1, backdrop, backdrop));
    expect(dismiss).toHaveBeenCalledTimes(1);

    controller.pointerDown(pointer(2, dialog, backdrop));
    controller.pointerUp(pointer(2, backdrop, backdrop));
    expect(dismiss).toHaveBeenCalledTimes(1);

    controller.pointerDown(pointer(3, backdrop, backdrop));
    controller.pointerUp(pointer(3, dialog, backdrop));
    expect(dismiss).toHaveBeenCalledTimes(1);
  });

  it("does not dismiss when dismissal is disabled before pointer release", () => {
    const dismiss = vi.fn();
    const backdrop = new EventTarget();
    let enabled = true;
    const controller = createBackdropDismissal(dismiss, () => enabled);

    controller.pointerDown(pointer(1, backdrop, backdrop));
    enabled = false;
    controller.pointerUp(pointer(1, backdrop, backdrop));

    expect(dismiss).not.toHaveBeenCalled();
  });
});
