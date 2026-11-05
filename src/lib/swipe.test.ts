import { describe, expect, it } from "vitest";
import { isSwipe, lockAxis, swipeOffset } from "./swipe";

describe("swipe on a row", () => {
  it("waits for a clear direction, then keeps it", () => {
    expect(lockAxis(4, 5)).toBeNull();
    expect(lockAxis(30, 4)).toBe("x");
    expect(lockAxis(-30, 4)).toBe("x");
    expect(lockAxis(6, 30)).toBe("y");
  });

  it("counts only a long, clearly sideways move", () => {
    expect(isSwipe(70, 0)).toBe(false);
    expect(isSwipe(120, 0)).toBe(true);
    expect(isSwipe(-120, 10)).toBe(true);
    expect(isSwipe(120, 80)).toBe(false);
  });

  it("stops the row at about 30% of its width", () => {
    expect(swipeOffset(50, 400)).toBe(50);
    expect(swipeOffset(300, 400)).toBe(120);
    expect(swipeOffset(-300, 400)).toBe(-120);
  });

  it("always lets the row pass the point of no return on a narrow screen", () => {
    expect(swipeOffset(200, 300)).toBe(96);
  });
});
