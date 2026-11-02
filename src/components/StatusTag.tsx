import type { OfferRow } from "../api/types";
import { useI18n } from "../i18n";
import { needsFollowUp } from "../lib/format";
import { Flap } from "./Flap";

/** Stage as a word on a flap tile (never colour alone) plus extra markers. */
export function StatusTag({ offer, fresh }: { offer: OfferRow; fresh?: boolean }) {
  const { t } = useI18n();
  return (
    <span className="status">
      <Flap className={`tag tag-${offer.stage}`} text={t(`stage.${offer.stage}`)} fresh={fresh} />
      {offer.expired && <span className="tag tag-flag">{t("flag.expired")}</span>}
      {needsFollowUp(offer) && <span className="tag tag-flag">{t("flag.noReply")}</span>}
    </span>
  );
}
