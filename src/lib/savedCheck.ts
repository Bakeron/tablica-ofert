import type { OfferDetail, OfferInput } from "../api/types";

/** What was saved differently than the form said - checked right after "Save offer". */
export type LostField = "title" | "company" | "url" | "city" | "remote" | "salary" | "contract" | "level" | "published" | "expires" | "description";

const text = (v: string | null | undefined) => {
  const t = (v ?? "").trim();
  return t === "" ? null : t;
};

/**
 * Fields of a newly added offer that the database does not hold the way the person confirmed
 * them on the screen. The rules are the ones the app saves by (`offers::clean`): trimmed text,
 * empty = none, the lower amount first, PLN and "a month" when a salary has no currency or period.
 */
export function lostFields(input: OfferInput, saved: OfferDetail): LostField[] {
  const lost: LostField[] = [];
  const same = (field: LostField, a: string | null | undefined, b: string | null | undefined) => {
    if (text(a) !== text(b)) lost.push(field);
  };
  same("title", input.title, saved.title);
  same("company", input.company, saved.company);
  same("url", input.url, saved.url);
  same("city", input.city, saved.city);
  if ((input.remote ?? "unknown") !== saved.remote) lost.push("remote");
  const a = input.salaryMin ?? null;
  const b = input.salaryMax ?? null;
  const [min, max] = a !== null && b !== null && a > b ? [b, a] : [a, b];
  const has = min !== null || max !== null;
  const currency = has ? (text(input.salaryCurrency)?.toUpperCase() ?? "PLN") : null;
  const period = has ? (input.salaryPeriod ?? "month") : null;
  if (min !== saved.salaryMin || max !== saved.salaryMax || currency !== saved.salaryCurrency || period !== saved.salaryPeriod) {
    lost.push("salary");
  }
  same("contract", input.contract, saved.contract);
  same("level", input.level, saved.level);
  // Dates compared as moments (the app may write them in another form); a publication date
  // in the future is saved as now.
  const time = (v: string | null | undefined) => (v ? Date.parse(v) : null);
  const published = time(input.publishedAt);
  if (published !== null && published <= Date.now() && published !== time(saved.publishedAt)) lost.push("published");
  if (time(input.expiresAt) !== time(saved.expiresAt)) lost.push("expires");
  same("description", input.description, saved.description);
  return lost;
}
