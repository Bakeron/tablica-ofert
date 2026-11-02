import type { Lang, T } from "../i18n";
import type { OfferRow } from "../api/types";

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

/** Compact salary for the board: "12-16 tys", "45-60 EUR/h". */
export function salaryShort(o: Pick<OfferRow, "salaryMin" | "salaryMax" | "salaryCurrency" | "salaryPeriod">, t: T): string {
  const { salaryMin: min, salaryMax: max } = o;
  if (min == null && max == null) return t("salary.none");
  const currency = o.salaryCurrency && o.salaryCurrency !== "PLN" ? ` ${o.salaryCurrency}` : "";
  if (o.salaryPeriod === "hour") {
    return `${range(min, max, (v) => String(v))}${currency}${t("salary.perHour")}`;
  }
  const k = (v: number) => {
    const thousands = v / 1000;
    return Number.isInteger(thousands) ? String(thousands) : thousands.toFixed(1).replace(".", ",");
  };
  return `${range(min, max, k)} ${t("salary.thousand")}${currency}`;
}

/** Full salary for the card: "12 000 - 16 000 PLN / miesiąc". */
export function salaryFull(o: Pick<OfferRow, "salaryMin" | "salaryMax" | "salaryCurrency" | "salaryPeriod">, lang: Lang, t: T): string {
  const { salaryMin: min, salaryMax: max } = o;
  if (min == null && max == null) return t("flag.noSalary");
  const nf = new Intl.NumberFormat(lang === "pl" ? "pl-PL" : "en-GB");
  const period = o.salaryPeriod === "hour" ? t("add.periodHour") : t("add.periodMonth");
  return `${range(min, max, (v) => nf.format(v))} ${o.salaryCurrency ?? "PLN"} / ${period}`;
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

/** 7 days after "applied" without any later stage → time for a follow-up. */
export function needsFollowUp(o: Pick<OfferRow, "stage" | "stageChangedAt">, now = Date.now()): boolean {
  return o.stage === "applied" && now - new Date(o.stageChangedAt).getTime() >= 7 * DAY;
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
