/**
 * Quick browsing (cards on the phone, keys 1 / 2 / 3 on the PC) keeps the order it started with:
 * a rating "Very" re-scores the list (learning from ratings), and the next offer must still be the
 * one that came next when browsing began - none skipped, none shown twice. The new order shows
 * once browsing ends (back to the list, another tab, a row chosen by hand).
 *
 * Rows from the start order come first, in that order; offers gone since are left out, and new
 * ones (e.g. a fetch meanwhile) go to the end in their own order.
 */
export function keepOrder<T extends { id: number }>(rows: T[], order: number[] | null): T[] {
  if (order === null) return rows;
  const place = new Map(order.map((id, i) => [id, i]));
  const known = rows.filter((r) => place.has(r.id)).sort((a, b) => place.get(a.id)! - place.get(b.id)!);
  return [...known, ...rows.filter((r) => !place.has(r.id))];
}
