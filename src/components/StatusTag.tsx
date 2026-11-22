import type { OfferRow } from "../api/types";
import { plural, useI18n } from "../i18n";
import { daysSince, isToday, needsFollowUp, WAITING_STAGES } from "../lib/format";
import { Flap } from "./Flap";

/** Stage as a word on a flap tile (never colour alone) plus extra markers. */
export function StatusTag({ offer, fresh }: { offer: OfferRow; fresh?: boolean }) {
  const { t, lang } = useI18n();
  // How long the person has waited for the company (the CV sent, the interview held...).
  const days = WAITING_STAGES.includes(offer.stage) ? daysSince(offer.stageChangedAt) : null;
  return (
    <span className="status">
      <Flap className={`tag tag-${offer.stage}`} text={t(`stage.${offer.stage}`)} fresh={fresh} />
      {days !== null && (
        <span className="status-age mono">{days === 0 ? t("stage.ago.today") : days === 1 ? t("time.yesterday") : t(plural(lang, "stage.ago", days), { count: days })}</span>
      )}
      {offer.nextEventKind === "interview" && offer.nextEvent && isToday(offer.nextEvent) && (
        <span className="tag tag-today">{t("flag.interviewToday")}</span>
      )}
      {offer.expired && <span className="tag tag-flag">{t("flag.expired")}</span>}
      {offer.stale && !offer.expired && !offer.archivedAt && <span className="tag tag-flag">{t("flag.stale")}</span>}
      {needsFollowUp(offer) && <span className="tag tag-flag">{t("flag.noReply")}</span>}
    </span>
  );
}
