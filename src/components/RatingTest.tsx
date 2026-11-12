import { useEffect, useState } from "react";
import type { Api } from "../api";
import type { RatingTestOffer, RatingTestVerdict } from "../api/types";
import { useI18n } from "../i18n";
import { isWebUrl, placeLabel, salaryShort } from "../lib/format";
import { IconArrowOut } from "./Icons";

const VERDICTS: RatingTestVerdict[] = ["yes", "maybe", "no"];
const WHY_MAX = 300;

/**
 * TEMPORARY - the author rates 10 offers blind (no AI score in sight), one at a time; the answers
 * are compared with the models in the test of AI ratings and the whole part is removed after it.
 */
export function RatingTest({ api, onOpenLink, onError }: { api: Api; onOpenLink: (url: string) => void; onError: (e: unknown) => void }) {
  const { t, lang } = useI18n();
  const [offers, setOffers] = useState<RatingTestOffer[] | null>(null);
  const [open, setOpen] = useState(false);
  const [at, setAt] = useState(0);
  const [verdict, setVerdict] = useState<RatingTestVerdict | null>(null);
  const [why, setWhy] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open && offers === null) api.ratingTestOffers().then(setOffers, onError);
  }, [open, offers, api, onError]);

  const offer = offers?.[at];
  // Each offer opens with the answer given before.
  useEffect(() => {
    setVerdict(offer?.answer?.verdict ?? null);
    setWhy(offer?.answer?.why ?? "");
  }, [offer]);

  const done = offers?.filter((o) => o.answer).length ?? 0;

  async function save() {
    if (!offer || !verdict) return;
    setBusy(true);
    try {
      await api.ratingTestAnswer(offer.id, verdict, why);
      const answered = { ...offer, answer: { verdict, why: why.trim() } };
      setOffers((list) => list && list.map((o) => (o.id === offer.id ? answered : o)));
      if (offers && at < offers.length - 1) setAt(at + 1);
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="rating-test" aria-labelledby="rating-test-title">
      <h3 id="rating-test-title">{t("ratingTest.title")}</h3>
      <p>{t("ratingTest.intro")}</p>
      {!open ? (
        <div>
          <button type="button" className="btn btn-sign" onClick={() => setOpen(true)}>
            {t("ratingTest.start")}
          </button>
        </div>
      ) : offers === null ? null : offers.length === 0 ? (
        <p className="hint">{t("ratingTest.empty")}</p>
      ) : (
        <>
          <p className="hint" aria-live="polite">
            {done === offers.length ? t("ratingTest.done") : t("ratingTest.progress", { done, count: offers.length })}
          </p>
          {offer && (
            <div className="rating-test-offer">
              <p className="rating-test-pos mono">{t("ratingTest.position", { n: at + 1, count: offers.length })}</p>
              <h4>{offer.title}</h4>
              <p>
                {[offer.company, placeLabel(offer, t), salaryShort(offer, lang, t)].filter(Boolean).join(" · ")}
              </p>
              {isWebUrl(offer.url) && (
                <div>
                  <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(offer.url!)}>
                    {t("ratingTest.open")} <IconArrowOut />
                  </button>
                </div>
              )}
              {/* Text only, never HTML: the description came from outside. */}
              <div className="rating-test-text" tabIndex={0} aria-label={t("ratingTest.description")}>
                {offer.description}
              </div>
              <fieldset className="rating-test-verdict">
                <legend>{t("ratingTest.verdict")}</legend>
                <div className="seg-strip">
                  {VERDICTS.map((v) => (
                    <button key={v} type="button" className="seg seg-radio" aria-pressed={verdict === v} onClick={() => setVerdict(v)}>
                      {t(`ratingTest.${v}`)}
                    </button>
                  ))}
                </div>
              </fieldset>
              <label className="field">
                <span>{t("ratingTest.why")}</span>
                <textarea rows={2} maxLength={WHY_MAX} value={why} onChange={(e) => setWhy(e.target.value)} />
              </label>
              <div className="rating-test-nav">
                <button type="button" className="btn btn-sign" onClick={save} disabled={!verdict || busy}>
                  {t("ratingTest.save")}
                </button>
                <button type="button" className="btn btn-ghost" onClick={() => setAt(at - 1)} disabled={at === 0}>
                  {t("ratingTest.prev")}
                </button>
                <button type="button" className="btn btn-ghost" onClick={() => setAt(at + 1)} disabled={at === offers.length - 1}>
                  {t("ratingTest.next")}
                </button>
                <button type="button" className="btn btn-ghost" onClick={() => setOpen(false)}>
                  {t("ratingTest.close")}
                </button>
              </div>
            </div>
          )}
        </>
      )}
    </section>
  );
}
