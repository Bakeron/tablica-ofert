/** How far the finger has to go before a row counts as swiped (px). */
export const SWIPE_AT = 80;
/** Below this the finger may still go either way (px). */
const LOCK_AT = 10;

export type SwipeAxis = "x" | "y";

/** Which way the finger went: the first clear direction decides the rest of the touch. */
export function lockAxis(dx: number, dy: number): SwipeAxis | null {
  if (Math.max(Math.abs(dx), Math.abs(dy)) < LOCK_AT) return null;
  return Math.abs(dx) > Math.abs(dy) ? "x" : "y";
}

/** A clear sideways swipe: right = "very", left = "no" (zalozenia.md §9). */
export function isSwipe(dx: number, dy: number): boolean {
  return Math.abs(dx) > SWIPE_AT && Math.abs(dx) > Math.abs(dy) * 2;
}

/** How far the row follows the finger: about 30% of its width, but always past the point of no return. */
export function swipeOffset(dx: number, width: number): number {
  const limit = Math.max(width * 0.3, SWIPE_AT + 16);
  return Math.max(-limit, Math.min(limit, dx));
}
