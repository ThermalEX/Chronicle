export type BackdropPointerEvent = Pick<PointerEvent, "pointerId" | "target" | "currentTarget">;

export interface BackdropDismissal {
  pointerDown(event: BackdropPointerEvent): void;
  pointerUp(event: BackdropPointerEvent): void;
  pointerCancel(): void;
}

export function createBackdropDismissal(onDismiss: () => void, canDismiss: () => boolean = () => true): BackdropDismissal {
  let backdropPointerId: number | undefined;

  return {
    pointerDown(event) {
      backdropPointerId = event.target === event.currentTarget ? event.pointerId : undefined;
    },
    pointerUp(event) {
      const shouldDismiss = backdropPointerId === event.pointerId && event.target === event.currentTarget && canDismiss();
      backdropPointerId = undefined;
      if (shouldDismiss) onDismiss();
    },
    pointerCancel() {
      backdropPointerId = undefined;
    },
  };
}
