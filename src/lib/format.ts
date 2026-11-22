import type { Lang, T } from "../i18n";
import type { OfferRow, SourceLink, Stage } from "../api/types";
import { FOLLOW_UP_DAYS } from "./limits";

const DAY = 86_400_000;

/** Board "Added" column: time today, "yesterday", or a short date. */
export function addedLabel(iso: string, lang: Lang, t: T, now = new Date()): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const locale = lang === "pl" ? "pl-PL" : "en-GB";
  if (date.getTime() >= startOfToday) {
    return date.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
  }
  if (date.getTime() >= startOfToday - DAY) return t("time.yesterday");
  return date.toLocaleDateString(locale, { day: "2-digit", month: "2-digit" });
}

/** The score as shown: with what the ratings taught, always 0-100 (`SHOWN_SCORE` in offers.rs). */
export function shownScore(offer: Pick<OfferRow, "score" | "learned">): number | null {
  return offer.score === null ? null : Math.min(100, Math.max(0, offer.score + offer.learned));
}

/** "+8" or "-6". */
export function signedPoints(points: number): string {
  return points > 0 ? `+${points}` : String(points);
}

export function fullDate(iso: string, lang: Lang): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleString(lang === "pl" ? "pl-PL" : "en-GB", {
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** A day without the hour - "open until 4 Dec 2026". */
export function fullDay(iso: string, lang: Lang): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleDateString(lang === "pl" ? "pl-PL" : "en-GB", { day: "numeric", month: "short", year: "numeric" });
}

/** A short day for the marks on the list - "5.11" / "5 Nov". */
export function shortDay(iso: string, lang: Lang): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleDateString(lang === "pl" ? "pl-PL" : "en-GB", { day: "numeric", month: lang === "pl" ? "numeric" : "short" });
}

/** Compact salary for the board: "12-16 tys.", "45-60 EUR/h". */
export function salaryShort(o: Pick<OfferRow, "salaryMin" | "salaryMax" | "salaryCurrency" | "salaryPeriod">, lang: Lang, t: T): string {
  const { salaryMin: min, salaryMax: max } = o;
  if (min == null && max == null) return t("salary.none");
  const currency = o.salaryCurrency && o.salaryCurrency !== "PLN" ? ` ${o.salaryCurrency}` : "";
  // An hour and a day in plain numbers, a month in thousands - each with its period.
  if (o.salaryPeriod === "hour" || o.salaryPeriod === "day") {
    return `${range(min, max, (v) => String(v))}${currency}${t(o.salaryPeriod === "hour" ? "salary.perHour" : "salary.perDay")}`;
  }
  const k = (v: number) => {
    const thousands = v / 1000;
    return Number.isInteger(thousands) ? String(thousands) : thousands.toFixed(1).replace(".", lang === "pl" ? "," : ".");
  };
  return `${range(min, max, k)} ${t("salary.thousand")}${currency}${t("salary.perMonth")}`;
}

/** Full salary for the card: "12 000 - 16 000 PLN / miesiąc". */
export function salaryFull(o: Pick<OfferRow, "salaryMin" | "salaryMax" | "salaryCurrency" | "salaryPeriod">, lang: Lang, t: T): string {
  const { salaryMin: min, salaryMax: max } = o;
  if (min == null && max == null) return t("flag.noSalary");
  const nf = new Intl.NumberFormat(lang === "pl" ? "pl-PL" : "en-GB");
  const period = o.salaryPeriod === "hour" ? t("add.periodHour") : o.salaryPeriod === "day" ? t("add.periodDay") : t("add.periodMonth");
  // No-break spaces: "PLN / miesiąc" stays together when the line wraps.
  return `${range(min, max, (v) => nf.format(v))} ${o.salaryCurrency ?? "PLN"} / ${period}`;
}

function range(min: number | null, max: number | null, fmt: (v: number) => string): string {
  if (min != null && max != null) return min === max ? fmt(min) : `${fmt(min)}-${fmt(max)}`;
  return fmt((min ?? max) as number);
}

/** City column: city, remote marker or both. */
export function placeLabel(o: Pick<OfferRow, "city" | "remote">, t: T): string {
  if (o.remote === "yes") return o.city ? `${t("remote.yes")} · ${o.city}` : t("remote.yes");
  if (o.remote === "hybrid") return o.city ? `${o.city} · ${t("remote.hybrid")}` : t("remote.hybrid");
  return o.city ?? t("card.missing");
}

/** An e-mail alert without a company name stores "? (a1b2c3)" (it keeps offers apart) - shown as words. */
export function companyLabel(company: string, t: T): string {
  return /^\? \([0-9a-f]+\)$/.test(company) ? t("card.unknownCompany") : company;
}

/** `FOLLOW_UP_DAYS` after "applied" without any later stage → time for a follow-up. */
export function needsFollowUp(o: Pick<OfferRow, "stage" | "stageChangedAt">, now = Date.now()): boolean {
  return o.stage === "applied" && now - new Date(o.stageChangedAt).getTime() >= FOLLOW_UP_DAYS * DAY;
}

/** Only plain web links may be opened - never `javascript:`, `file:` etc. */
export function isWebUrl(value: string | null | undefined): value is string {
  if (!value) return false;
  try {
    const url = new URL(value);
    return url.protocol === "https:" || url.protocol === "http:";
  } catch {
    return false;
  }
}

/** One web link per city, when the same job came for several cities (otherwise one "Open offer"). */
export function linksByCity(links: SourceLink[]): { city: string; url: string }[] {
  const out: { city: string; url: string }[] = [];
  for (const l of links) {
    const city = l.city?.trim();
    if (city && isWebUrl(l.url) && !out.some((o) => o.city.toLowerCase() === city.toLowerCase())) {
      out.push({ city, url: l.url });
    }
  }
  return out;
}

/** Is this moment today on the person's own clock? */
export function isToday(iso: string, now = new Date()): boolean {
  const d = new Date(iso);
  return !Number.isNaN(d.getTime()) && d.toDateString() === now.toDateString();
}

/** "2026-11-05T14:00" (local, for <input type="datetime-local">) -> UTC RFC 3339. */
export function localInputToUtc(value: string): string | null {
  const d = new Date(value);
  return Number.isNaN(d.getTime()) ? null : d.toISOString().replace(/\.\d{3}Z$/, "Z");
}

/** Now as a value for <input type="datetime-local">. */
export function nowLocalInput(now = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T${pad(now.getHours())}:${pad(now.getMinutes())}`;
}

/** The title translated by AI into the app language, when there is one. */
export function translatedTitle(o: Pick<OfferRow, "titleTranslated" | "translationLang">, lang: Lang): string | null {
  return o.translationLang === lang && o.titleTranslated ? o.titleTranslated : null;
}

/** The local day of a moment as a value for <input type="date"> ("2026-11-05"). */
export function localDayInput(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** The same moment moved to another local day (the hour stays), never later than now - UTC, RFC 3339. */
export function onLocalDay(iso: string, day: string, now = new Date()): string | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  if (!m) return null;
  const base = new Date(iso);
  const d = Number.isNaN(base.getTime()) ? new Date(now) : base;
  d.setFullYear(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  if (Number.isNaN(d.getTime()) || d.getFullYear() < 2000) return null;
  const at = d.getTime() > now.getTime() ? now : d;
  return at.toISOString().replace(/\.\d{3}Z$/, "Z");
}

/** Whole days on the person's own calendar from a moment to now (0 = today). */
export function daysSince(iso: string, now = new Date()): number | null {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return null;
  const day = (x: Date) => Date.UTC(x.getFullYear(), x.getMonth(), x.getDate());
  return Math.max(0, Math.round((day(now) - day(d)) / DAY));
}

/** Stages where the person waits for the company - the list shows how long ("12 days ago"). */
export const WAITING_STAGES: readonly Stage[] = ["applied", "replied", "hr_interview", "tech_stage", "job_offer"];
