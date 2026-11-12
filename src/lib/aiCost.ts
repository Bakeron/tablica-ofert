/** Rough cost of one offer read by Claude (fields and score in one request), in PLN - from the
 *  test of 11.11.2026 (Sonnet 5.5, effort low: about $0.008-0.011 a rating, a full page a bit
 *  more; about 4 PLN to the dollar). Opus costs twice as much, Haiku 20 times less; an unknown
 *  model is counted as Opus, so the estimate is never too low. */
export function claudeOfferCost(model: string): number {
  const m = model.toLowerCase();
  if (m.includes("haiku")) return 0.0025;
  if (m.includes("sonnet")) return 0.05;
  return 0.1;
}

/** "1,20" - the estimate for `count` offers, rounded up to 10 grosze (never shown as 0). */
export function claudeCostText(count: number, model: string, lang: string): string {
  const tenths = Math.max(1, Math.ceil(count * claudeOfferCost(model) * 10 - 1e-9));
  return (tenths / 10).toLocaleString(lang === "pl" ? "pl-PL" : "en-GB", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

/** Rough dollar rate for the spending counter (PLN per $1, as of 11.2026) - shown only as "about". */
export const USD_PLN = 4;

/** A typed top-up as stored ("5", "20.50"): without "$" / "USD" and spaces, a comma as a dot.
 *  `null` = not a sum the settings accept (0.01 to 10000, at most two digits after the dot);
 *  `""` = the field was cleared. */
export function topUpTyped(text: string): string | null {
  const v = text.replace(/\$|usd/gi, "").replace(/\s/g, "").replace(",", ".");
  if (v === "") return "";
  if (!/^\d+(\.\d{1,2})?$/.test(v)) return null;
  const n = Number(v);
  return n >= 0.01 && n <= 10000 ? v : null;
}

/** "1,23 $" / "$1.23" - spending in dollars; a few calls under a cent are not shown as zero. */
export function usdText(usd: number, lang: string): string {
  const pl = lang === "pl";
  const small = usd > 0 && usd < 0.005;
  const n = (small ? 0.01 : Math.max(0, usd)).toLocaleString(pl ? "pl-PL" : "en-GB", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  const text = pl ? `${n} $` : `$${n}`;
  return small ? `< ${text}` : text;
}

/** "20,00" - dollars in PLN at the rough rate. */
export function plnText(usd: number, lang: string): string {
  return (usd * USD_PLN).toLocaleString(lang === "pl" ? "pl-PL" : "en-GB", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

/** Today as `YYYY-MM-DD` in local time (the day of a top-up). */
export function localDay(now = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}
