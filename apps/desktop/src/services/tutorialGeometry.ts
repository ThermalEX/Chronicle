export function placeTutorialCard(rect: { left: number; top: number; right: number; bottom: number }, width: number, height: number, cardWidth: number, cardHeight: number): { left: number; top: number } {
  const gap = 12;
  let left = rect.left;
  let top = rect.bottom + gap;
  if (top + cardHeight > height - gap) {
    if (rect.top - gap - cardHeight >= gap) top = rect.top - gap - cardHeight;
    else if (rect.right + gap + cardWidth <= width - gap) { left = rect.right + gap; top = rect.top; }
    else if (rect.left - gap - cardWidth >= gap) { left = rect.left - gap - cardWidth; top = rect.top; }
    else top = height - cardHeight - gap;
  }
  return { left: Math.max(gap, Math.min(left, width - cardWidth - gap)), top: Math.max(gap, Math.min(top, height - cardHeight - gap)) };
}
