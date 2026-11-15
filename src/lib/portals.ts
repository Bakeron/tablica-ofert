import { STAGES, type OfferRow, type Remote } from "../api/types";

/** Portals of an offer and of every ad under it in the tree (the names the list shows). */
export function rowPortals(row: Pick<OfferRow, "sources" | "children">): string[] {
  return [...new Set([...row.sources, ...row.children.flatMap((c) => c.sources)])];
}

/** The choices above every list: portal, status, city / work mode, salary, English. `null` = all. */
export type FilterKey = "portal" | "status" | "place" | "salary" | "english";
export type ListFilters = Record<FilterKey, string | null>;
export const FILTER_KEYS: FilterKey[] = ["portal", "status", "place", "salary", "english"];
export const NO_FILTERS: ListFilters = { portal: null, status: null, place: null, salary: null, english: null };

/** "Salary": given at all, not given, per month, per day, per hour. */
export const SALARY_CHOICES = ["any", "none", "month", "day", "hour"] as const;
/** "English": required, a plus, not named. */
const ENGLISH_CHOICES = ["required", "nice", "none"];
const REMOTES: Remote[] = ["yes", "hybrid", "no", "unknown"];

type Row = Pick<OfferRow, "sources" | "children" | "stage" | "city" | "remote" | "salaryMin" | "salaryMax" | "salaryPeriod" | "english">;

/** The values an offer has for one filter. A family counts with every ad (portal, city, work mode);
 *  the status is the one the list shows. "Place" values: `remote:<mode>` and `city:<name>`.
 *  English: what any ad of the family says (one "required" is enough to show it there). */
export function rowValues(row: Row, key: FilterKey): string[] {
  switch (key) {
    case "portal":
      return rowPortals(row);
    case "status":
      return [row.stage];
    case "place": {
      const ads = [row, ...row.children];
      return [...new Set([...ads.map((a) => `remote:${a.remote}`), ...ads.flatMap((a) => (a.city ? [`city:${a.city}`] : []))])];
    }
    case "salary":
      return row.salaryMin == null && row.salaryMax == null ? ["none"] : ["any", row.salaryPeriod ?? "month"];
    case "english":
      return [...new Set([row, ...row.children].map((a) => a.english ?? "none"))];
  }
}

/** The offers that match every chosen filter (`except` - one left out, for its own counts). */
export function applyFilters<T extends Row>(rows: T[], filters: ListFilters, except?: FilterKey): T[] {
  const keys = FILTER_KEYS.filter((k) => k !== except && filters[k] !== null);
  return keys.length === 0 ? rows : rows.filter((r) => keys.every((k) => rowValues(r, k).includes(filters[k]!)));
}

/** How many offers each value of one filter has - on the list narrowed by the other filters.
 *  The chosen value stays with 0 when the list has none, so the filter never hides itself.
 *  Order: portals and cities the biggest first, statuses and work modes as everywhere in the app. */
export function filterCounts(rows: Row[], filters: ListFilters, key: FilterKey): { value: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const r of applyFilters(rows, filters, key)) for (const v of rowValues(r, key)) counts.set(v, (counts.get(v) ?? 0) + 1);
  const chosen = filters[key];
  if (chosen !== null && !counts.has(chosen)) counts.set(chosen, 0);
  const rank = (v: string): number => {
    if (key === "status") return STAGES.indexOf(v as (typeof STAGES)[number]);
    if (key === "salary") return SALARY_CHOICES.indexOf(v as (typeof SALARY_CHOICES)[number]);
    if (key === "english") return ENGLISH_CHOICES.indexOf(v);
    if (key === "place" && v.startsWith("remote:")) return REMOTES.indexOf(v.slice(7) as Remote) - REMOTES.length;
    return 0;
  };
  return [...counts]
    .map(([value, count]) => ({ value, count }))
    .sort((a, b) => rank(a.value) - rank(b.value) || (key === "status" || key === "salary" || key === "english" ? 0 : b.count - a.count) || a.value.localeCompare(b.value));
}
