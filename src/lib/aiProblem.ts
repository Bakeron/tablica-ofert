import type { AiProblem, LowFunds, RateReport } from "../api/types";
import type { TextKey } from "../i18n";
import { usdText } from "./aiCost";

type T = (key: TextKey, params?: Record<string, string | number>) => string;

/** "Google Gemini (gemini-3.5-flash-lite): the model is not available for this account…" */
export function problemLine(p: AiProblem, t: T): string {
  return t("ai.problemLine", {
    name: t(`ai.p.${p.provider}` as TextKey),
    model: p.model,
    why: t(`ai.problem.${p.kind}` as TextKey),
  });
}

/** "About $0.40 of the $5.00 Claude top-up is left…" */
export function lowFundsLine(low: LowFunds, t: T, lang: string): string {
  return t("ai.lowFunds", { left: usdText(low.left, lang), topUp: usdText(low.topUp, lang) });
}

/** The run left collected offers waiting: Claude is the only provider on (`ai::NO_FREE`). */
export const noFreeProvider = (report: RateReport) => report.error?.code === "missing" && report.error.detail.endsWith("free ai provider");

/** One sentence about an AI run, with every provider that failed and why. */
export function rateSummary(report: RateReport, t: T, lang: string): string {
  const parts: string[] = [];
  if (report.stopped) {
    parts.push(t("ai.stoppedByYou", { count: report.rated, waiting: report.waiting }));
  } else if (noFreeProvider(report)) {
    parts.push(t("ai.rated", { count: report.rated, waiting: report.waiting }), t("ai.noFree"));
  } else if (report.error) {
    parts.push(t("ai.stopped"));
    // An error that came before any provider was asked (e.g. a busy run) has no problem line.
    if (report.problems.length === 0) {
      const known = ["net", "auth", "limit", "missing", "busy"];
      // "Busy" here is another AI run (a fetch or sync has its own message).
      const code = report.error.code === "busy" ? "busyAi" : report.error.code;
      parts.push(t((known.includes(report.error.code) ? `error.${code}` : "error.generic") as TextKey));
    }
  } else {
    parts.push(t("ai.rated", { count: report.rated, waiting: report.waiting }));
  }
  parts.push(...report.problems.map((p) => problemLine(p, t)));
  if (report.noProfile > 0) parts.push(t("ai.noProfile", { count: report.noProfile }));
  if (report.lowFunds) parts.push(lowFundsLine(report.lowFunds, t, lang), t("ai.lowFundsWhere"));
  // "AI rated…" has no full stop of its own - it gets one when more sentences follow.
  return parts.map((part, i) => (i < parts.length - 1 && !/[.!?…]$/.test(part) ? `${part}.` : part)).join(" ");
}
