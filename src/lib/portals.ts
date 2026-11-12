import type { OfferRow } from "../api/types";

/** Portals of an offer and of every ad under it in the tree (the names the list shows). */
export function rowPortals(row: Pick<OfferRow, "sources" | "children">): string[] {
  return [...new Set([...row.sources, ...row.children.flatMap((c) => c.sources)])];
}

/** The offers with at least one ad from `portal` (a job seen on two portals shows under both); `null` = all. */
export function byPortal<T extends Pick<OfferRow, "sources" | "children">>(rows: T[], portal: string | null): T[] {
  return portal === null ? rows : rows.filter((r) => rowPortals(r).includes(portal));
}

/** How many offers of the list each portal has, the biggest first (then by name). The chosen
 *  portal stays on the list with 0 when this list has none - so the filter never hides itself. */
export function portalCounts(rows: Pick<OfferRow, "sources" | "children">[], chosen: string | null): { portal: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const r of rows) for (const p of rowPortals(r)) counts.set(p, (counts.get(p) ?? 0) + 1);
  if (chosen !== null && !counts.has(chosen)) counts.set(chosen, 0);
  return [...counts].map(([portal, count]) => ({ portal, count })).sort((a, b) => b.count - a.count || a.portal.localeCompare(b.portal));
}
