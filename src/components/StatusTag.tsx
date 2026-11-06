import type { OfferRow } from "../api/types";
import { useI18n } from "../i18n";
import { isToday, needsFollowUp } from "../lib/format";
import { Flap } from "./Flap";

/** Stage as a word on a flap tile (never colour alone) plus extra markers. */
export function StatusTag({ offer, fresh }: { offer: OfferRow; fresh?: boolean }) {
  const { t } = useI18n();
  return (
    <span className="status">
      <Flap className={`tag tag-${offer.stage}`} text={t(`stage.${offer.stage}`)} fresh={fresh} />
      {offer.nextEventKind === "interview" && offer.nextEvent && isToday(offer.nextEvent) && (
        <span className="tag tag-today">{t("flag.interviewToday")}</span>
      )}
      {offer.expired && <span className="tag tag-flag">{t("flag.expired")}</span>}
      {offer.stale && !offer.expired && !offer.archivedAt && <span className="tag tag-flag">{t("flag.stale")}</span>}
      {needsFollowUp(offer) && <span className="tag tag-flag">{t("flag.noReply")}</span>}
    </span>
  );
}
