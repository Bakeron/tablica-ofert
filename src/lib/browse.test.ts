import { describe, expect, it } from "vitest";
import { keepOrder } from "./browse";

const rows = (...ids: number[]) => ids.map((id) => ({ id }));
const ids = (list: { id: number }[]) => list.map((r) => r.id);

describe("keepOrder", () => {
  it("leaves the list as it is when nobody is browsing", () => {
    expect(ids(keepOrder(rows(3, 1, 2), null))).toEqual([3, 1, 2]);
  });

  it("keeps the start order after a re-score, so the next offer is still the next one", () => {
    // Started with 1, 2, 3, 4; rating 1 "Very" moved 4 to the top.
    const shown = keepOrder(rows(4, 1, 3, 2), [1, 2, 3, 4]);
    expect(ids(shown)).toEqual([1, 2, 3, 4]);
    expect(shown[shown.findIndex((r) => r.id === 1) + 1].id).toBe(2);
  });

  it("skips offers that left the list and puts new ones at the end", () => {
    expect(ids(keepOrder(rows(9, 4, 3, 8, 1), [1, 2, 3, 4]))).toEqual([1, 3, 4, 9, 8]);
  });
});
