export type FloatingMenuRect = Pick<DOMRect, "top" | "right" | "bottom" | "left" | "width" | "height">;

export type FloatingMenuViewport = Pick<DOMRect, "width" | "height">;

export type FloatingMenuPosition = {
  top: number;
  left: number;
  minWidth: number;
  maxHeight?: number;
};

export type FloatingMenuStyle = {
  top: string;
  left: string;
  minWidth: string;
  maxHeight?: string;
};

const MENU_GAP = 5;
const VIEWPORT_MARGIN = 8;

/** Positions a menu against the viewport, so scroll containers cannot clip it. */
export function positionFloatingMenu(
  trigger: FloatingMenuRect,
  menu: Pick<FloatingMenuRect, "width" | "height">,
  viewport: FloatingMenuViewport,
): FloatingMenuPosition {
  const availableHeight = Math.max(0, viewport.height - VIEWPORT_MARGIN * 2);
  const height = Math.min(menu.height, availableHeight);
  const below = trigger.bottom + MENU_GAP;
  const above = trigger.top - height - MENU_GAP;
  const top = below + height <= viewport.height - VIEWPORT_MARGIN
    ? below
    : Math.max(VIEWPORT_MARGIN, above);
  const left = Math.max(
    VIEWPORT_MARGIN,
    Math.min(trigger.left, viewport.width - menu.width - VIEWPORT_MARGIN),
  );

  return {
    top, left, minWidth: trigger.width,
    ...(menu.height > availableHeight ? { maxHeight: availableHeight } : {}),
  };
}

/** Converts menu geometry into CSS values accepted by the browser. */
export function floatingMenuStyle(position: FloatingMenuPosition): FloatingMenuStyle {
  return {
    top: `${position.top}px`,
    left: `${position.left}px`,
    minWidth: `${position.minWidth}px`,
    ...(position.maxHeight === undefined ? {} : { maxHeight: `${position.maxHeight}px` }),
  };
}
