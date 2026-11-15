import { STAGES, type OfferRow, type Remote } from "../api/types";

/** Portals of an offer and of every ad under it in the tree (the names the list shows). */
export function rowPortals(row: Pick<OfferRow, "sources" | "children">): string[] {
  return [...new Set([...row.sources, ...row.children.flatMap((c) => c.sources)])];
}

/** The choices above every list. Each holds the chosen values - none = all;
 *  status takes one (a list), the others any number (tick boxes). English is one tick box:
 *  none = offers that need English stay hidden, `["all"]` = shown too. */
export type FilterKey = "portal" | "status" | "mode" | "city" | "salary" | "english";
export type ListFilters = Record<FilterKey, string[]>;
export const FILTER_KEYS: FilterKey[] = ["portal", "status", "mode", "city", "salary", "english"];
export const NO_FILTERS: ListFilters = { portal: [], status: [], mode: [], city: [], salary: [], english: [] };
/** Filters with tick boxes - several values at once (status is one choice from a list; English is one tick box). */
export const MULTI_KEYS: FilterKey[] = ["portal", "mode", "city", "salary"];
/** The English tick box ticked: offers that need English are shown too. */
export const ENGLISH_ALL = "all";

/** "Salary": not given, per month, per day, per hour. */
export const SALARY_CHOICES = ["none", "month", "day", "hour"] as const;
const REMOTES: Remote[] = ["yes", "hybrid", "no", "unknown"];

type Row = Pick<OfferRow, "sources" | "children" | "stage" | "city" | "remote" | "salaryMin" | "salaryMax" | "salaryPeriod" | "english">;

/** The values an offer has for one filter. A family counts with every ad (portal, work mode, city,
 *  English); the status and the salary are the ones the list shows. */
export function rowValues(row: Row, key: FilterKey): string[] {
  const ads = [row, ...row.children];
  switch (key) {
    case "portal":
      return rowPortals(row);
    case "status":
      return [row.stage];
    case "mode":
      return [...new Set(ads.map((a) => a.remote))];
    case "city":
      return [...new Set(ads.flatMap((a) => (a.city ? [a.city] : [])))];
    case "salary":
      return row.salaryMin == null && row.salaryMax == null ? ["none"] : [row.salaryPeriod ?? "month"];
    case "english":
      return [...new Set(ads.map((a) => a.english ?? "none"))];
  }
}

/** An offer that needs English - every ad of the family names it as required (an ad in English counts too). */
export function needsEnglish(row: Row): boolean {
  return rowValues(row, "english").every((v) => v === "required");
}

/** The offers that match every filter: any of its chosen values (`except` - one left out, for its own counts).
 *  English unticked hides the offers that need it. */
export function applyFilters<T extends Row>(rows: T[], filters: ListFilters, except?: FilterKey): T[] {
  const keys = FILTER_KEYS.filter((k) => k !== except && k !== "english" && filters[k].length > 0);
  const hideEnglish = except !== "english" && !filters.english.includes(ENGLISH_ALL);
  if (keys.length === 0 && (!hideEnglish || !rows.some(needsEnglish))) return rows;
  return rows.filter((r) => keys.every((k) => rowValues(r, k).some((v) => filters[k].includes(v))) && !(hideEnglish && needsEnglish(r)));
}

/** How many offers each value of one filter has - on the list narrowed by the other filters.
 *  Every value of the whole list stays as an option, with 0 when the other filters leave none
 *  (choosing one filter never makes the others vanish); chosen values stay with 0 too.
 *  Order: portals and cities the biggest first, statuses, work modes and the rest as in the app. */
export function filterCounts(rows: Row[], filters: ListFilters, key: FilterKey): { value: string; count: number }[] {
  // English: one choice - how many offers that need it the tick box would add.
  if (key === "english") return [{ value: ENGLISH_ALL, count: applyFilters(rows, filters, "english").filter(needsEnglish).length }];
  const counts = new Map<string, number>();
  for (const r of rows) for (const v of rowValues(r, key)) counts.set(v, 0);
  for (const r of applyFilters(rows, filters, key)) for (const v of rowValues(r, key)) counts.set(v, (counts.get(v) ?? 0) + 1);
  for (const v of filters[key]) if (!counts.has(v)) counts.set(v, 0);
  const order: Partial<Record<FilterKey, readonly string[]>> = { status: STAGES, mode: REMOTES, salary: SALARY_CHOICES };
  const fixed = order[key];
  return [...counts]
    .map(([value, count]) => ({ value, count }))
    .sort((a, b) => (fixed ? fixed.indexOf(a.value) - fixed.indexOf(b.value) : b.count - a.count || a.value.localeCompare(b.value)));
}
