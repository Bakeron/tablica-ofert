import type { Criteria, FilterResult, OfferRow, Reason, View } from "../api/types";
import type { T, TextKey } from "../i18n";
import { SALARY_FAR_ABOVE } from "./limits";

/** Criteria of a new profile: nothing required yet, everything except keywords as a hard filter. */
export function emptyCriteria(): Criteria {
  return {
    keywords: [],
    workModes: [],
    cities: [],
    commutePerMonth: null,
    contracts: [],
    levels: [],
    languages: [],
    salaryUop: { min: null, expected: null, basis: "gross", period: "month" },
    salaryB2b: { min: null, expected: null, basis: "net", period: "month" },
    skipCompanies: [],
    excludeWords: [],
    hard: {
      location: true,
      contract: true,
      salary: true,
      level: true,
      language: true,
      companies: true,
      excludeWords: true,
      keywords: false,
    },
  };
}

/** "React, TypeScript; Node.js" or one entry per line → ["React", "TypeScript", "Node.js"]. */
export function splitList(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const part of text.split(/[,;\n]/)) {
    const value = part.trim();
    if (value && !seen.has(value.toLocaleLowerCase())) {
      seen.add(value.toLocaleLowerCase());
      out.push(value);
    }
  }
  return out;
}

export const joinList = (values: string[]) => values.join(", ");

/** "12 000" / "12000" / "12,5" → whole number, empty → null. */
export function toAmount(value: string): number | null {
  const digits = value.replace(/[\s ]/g, "").replace(",", ".");
  if (digits === "") return null;
  const n = Math.round(Number(digits));
  return Number.isFinite(n) && n >= 0 ? n : null;
}

/** One line of text for a filter reason, e.g. "umowa: B2B". */
export function reasonText(reason: Reason, t: T): string {
  const key = `filter.reason.${reason.code}` as TextKey;
  const value = reason.code === "language_mismatch" ? reason.value?.toUpperCase() : reason.value;
  const text = t(key, { value: value ?? "", times: SALARY_FAR_ABOVE });
  return text === key ? reason.code : text.replace(/\s*\(\)\s*$/, "").trim();
}

/** Reasons that decided the result go first; notes ("no salary range") after them. */
export function sortedReasons(reasons: Reason[]): Reason[] {
  const order = { fail: 0, unsure: 1, note: 2 } as const;
  return [...reasons].sort((a, b) => order[a.kind] - order[b.kind]);
}

/** The user already acted on the offer - it stays on the board whatever the filter says (as in Rust). */
export function isKept(o: Pick<OfferRow, "stage" | "favorite" | "interest">): boolean {
  return o.stage !== "new" || o.favorite || o.interest === "high" || o.interest === "maybe";
}

/** The lists inside "Collected" (the switch). */
const COLLECTED_LISTS: readonly View[] = ["today", "board", "review", "rejected"];

/** The lists inside "Bin" (the switch): all, not for me, expired (the archive), deleted. */
export const BIN = ["bin", "declined", "archive", "trash"] as const;
export type BinView = (typeof BIN)[number];
export const isBin = (v: View): v is BinView => (BIN as readonly View[]).includes(v);

/** Where an offer is, in words: "Own", "Collected - All", "Bin - Not for me"... (messages, CSV). */
export function placeName(view: View, t: T): string {
  if (COLLECTED_LISTS.includes(view)) return t("place.collected", { list: t(`collected.${view}` as TextKey) });
  if (isBin(view) && view !== "bin") return t("place.bin", { list: t(`bin.${view}`) });
  return t(`tabs.${view}` as TextKey);
}

/** Tab an offer lands in, for its filter result ("no" wins over everything, as in Rust). */
export function viewFor(o: Pick<OfferRow, "stage" | "favorite" | "interest" | "filterResult" | "own">) {
  if (o.interest === "no") return "declined" as const;
  if (o.own) return "own" as const;
  if (isKept(o) || o.filterResult === "pass") return "board" as const;
  return o.filterResult === "unsure" ? ("review" as const) : ("rejected" as const);
}

/** The tab an offer goes to after a change, or `null` when it stays in `view` (as the tabs in
 *  offers.rs: "Today" keeps fresh offers until "no", a recruitment or "expired"; the bin keeps
 *  expired and deleted ones until "Restore", a "not for me" one until the interest changes). */
export function leavesView(
  o: Pick<OfferRow, "stage" | "favorite" | "interest" | "filterResult" | "expired" | "own"> & Partial<Pick<OfferRow, "bin">>,
  view: View,
): View | null {
  const target = viewFor(o);
  if (isBin(view)) return o.bin === "declined" && o.interest !== "no" ? target : null;
  if (view !== "today") return target === view ? null : target;
  const recruiting = o.stage !== "new" && o.stage !== "interested";
  const rejected = o.filterResult === "fail" && !isKept(o);
  return o.interest === "no" || o.own || recruiting || o.expired || rejected ? target : null;
}

/** Best of several profiles: one profile that fits is enough (as `filter::best` in Rust). */
export function bestResult(results: FilterResult[]): FilterResult {
  if (results.length === 0 || results.includes("pass")) return "pass";
  return results.includes("unsure") ? "unsure" : "fail";
}
