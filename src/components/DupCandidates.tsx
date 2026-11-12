import type { ReactNode } from "react";
import type { DupCandidate, DupVerdict } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { companyLabel, fullDay, salaryFull } from "../lib/format";
import { sourceLabel } from "./Board";

/** AI's verdicts: `null` = not asked (no AI), "wait" = asking, "error" = no answer. */
export type Verdicts = DupVerdict[] | "wait" | "error" | null;

/** "Probably the same" offers the code found, with what agrees and AI's word about each. The
 *  buttons (`actions`) are the caller's - nothing is joined on AI's word alone. */
export function DupCandidates({
  candidates,
  verdicts,
  actions,
}: {
  candidates: DupCandidate[];
  verdicts: Verdicts;
  actions: (c: DupCandidate) => ReactNode;
}) {
  const { t, lang } = useI18n();
  return (
    <ol className="dup-list">
      {candidates.map((c) => {
        const w = c.was;
        const verdict = Array.isArray(verdicts) ? verdicts.find((v) => v.id === w.id) : undefined;
        const salary = w.salaryMin !== null || w.salaryMax !== null ? salaryFull(w, lang, t) : null;
        return (
          <li key={w.id} className="dup-item">
            <p className="dup-name">
              „{w.title}” - {companyLabel(w.company, t)}
            </p>
            <p className="dup-facts">
              {[w.city, salary, c.sources.map((s) => sourceLabel(s, t)).join(" · ")].filter(Boolean).join(" · ")}
            </p>
            <p className="dup-why">
              {t("dup.agrees", { list: c.reasons.map((r) => t(`dup.reason.${r}` as TextKey)).join(", ") })}
            </p>
            {w.sentVia && <p className="same-applied">{t("same.sentVia", { date: fullDay(w.sentVia.at, lang), source: sourceLabel(w.sentVia.source, t) })}</p>}
            {verdicts === "wait" && <p className="dup-ai">{t("dup.aiWait")}</p>}
            {verdicts === "error" && <p className="dup-ai">{t("dup.aiError")}</p>}
            {verdict && (
              <p className={`dup-ai dup-ai-${verdict.same}`}>
                <span className="dup-ai-mark">{t(`dup.ai.${verdict.same}`)}</span> {verdict.why}
              </p>
            )}
            <div className="dup-actions">{actions(c)}</div>
          </li>
        );
      })}
    </ol>
  );
}
