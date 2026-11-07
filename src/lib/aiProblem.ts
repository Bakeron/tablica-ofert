import type { AiProblem, RateReport } from "../api/types";
import type { TextKey } from "../i18n";

type T = (key: TextKey, params?: Record<string, string | number>) => string;

/** "Google Gemini (gemini-3.5-flash-lite): the model is not available for this account…" */
export function problemLine(p: AiProblem, t: T): string {
  return t("ai.problemLine", {
    name: t(`ai.p.${p.provider}` as TextKey),
    model: p.model,
    why: t(`ai.problem.${p.kind}` as TextKey),
  });
}

/** One sentence about an AI run, with every provider that failed and why. */
export function rateSummary(report: RateReport, t: T): string {
  const parts: string[] = [];
  if (report.error) {
    parts.push(t("ai.stopped"));
    // An error that came before any provider was asked (e.g. a busy run) has no problem line.
    if (report.problems.length === 0) {
      const known = ["net", "auth", "limit", "missing", "busy"];
      parts.push(t((known.includes(report.error.code) ? `error.${report.error.code}` : "error.generic") as TextKey));
    }
  } else {
    parts.push(t("ai.rated", { count: report.rated, waiting: report.waiting }));
  }
  parts.push(...report.problems.map((p) => problemLine(p, t)));
  if (report.noProfile > 0) parts.push(t("ai.noProfile", { count: report.noProfile }));
  // "AI rated…" has no full stop of its own - it gets one when more sentences follow.
  return parts.map((part, i) => (i < parts.length - 1 && !/[.!?…]$/.test(part) ? `${part}.` : part)).join(" ");
}
