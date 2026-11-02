import { forwardRef, useEffect, useState } from "react";
import { STAGES, type AssistKind, type Interest, type OfferDetail, type ProfileFilter, type Stage } from "../api/types";
import { useI18n } from "../i18n";
import { fullDate, isWebUrl, placeLabel, salaryFull } from "../lib/format";
import { reasonText, sortedReasons } from "../lib/profile";
import { sourceLabel } from "./Board";
import { IconArrowOut, IconRestore, IconStar, IconTrash, IconWarning } from "./Icons";
import { StatusTag } from "./StatusTag";

interface Props {
  offer: OfferDetail | null;
  onStage: (stage: Stage) => void;
  onInterest: (interest: Interest | null) => void;
  onFavorite: (favorite: boolean) => void;
  onNotes: (notes: string) => void;
  onOpenLink: (url: string) => void;
  onTrash: () => void;
  onRestore: () => void;
  onPurge: () => void;
  /** An AI provider is set. */
  aiOn: boolean;
  onRateAgain: () => void;
  onAssist: (kind: AssistKind) => void;
  /** Opens the offer page in the preview window to take its full text. */
  onFullText: () => void;
}

/** Offer details shaped like a boarding pass: key data on the stub above the perforation. */
export const OfferCard = forwardRef<HTMLHeadingElement, Props>(function OfferCard(
  { offer, onStage, onInterest, onFavorite, onNotes, onOpenLink, onTrash, onRestore, onPurge, aiOn, onRateAgain, onAssist, onFullText },
  headingRef,
) {
  const { t, lang } = useI18n();
  const [notes, setNotes] = useState(offer?.notes ?? "");

  useEffect(() => setNotes(offer?.notes ?? ""), [offer?.id, offer?.notes]);

  if (!offer) {
    return (
      <aside className="pass pass-empty" aria-label={t("card.title")}>
        <p>{t("card.empty")}</p>
      </aside>
    );
  }

  const inTrash = offer.deletedAt !== null;
  const missing = t("card.missing");
  const remoteText = t(`remote.${offer.remote}`);

  return (
    <aside className="pass" aria-labelledby="pass-title">
      {/* Stub - the part you would tear off */}
      <section className="pass-stub">
        <div className="pass-field pass-field-wide">
          <span className="pass-label">{t("card.title")}</span>
          <h2 id="pass-title" ref={headingRef} tabIndex={-1} className="pass-title">
            {offer.title}
          </h2>
        </div>
        <div className="pass-field pass-field-wide">
          <span className="pass-label">{t("card.company")}</span>
          <span className="pass-company">{offer.company}</span>
        </div>
        <div className="pass-grid">
          <div className="pass-field">
            <span className="pass-label">{t("card.salary")}</span>
            <span className="pass-value">{salaryFull(offer, lang, t)}</span>
          </div>
          <div className="pass-field">
            <span className="pass-label">{t("card.place")}</span>
            <span className="pass-value">{placeLabel(offer, t)}</span>
          </div>
          <div className="pass-field">
            <span className="pass-label">{t("remote.label")}</span>
            <span className="pass-value">{remoteText}</span>
          </div>
          <div className="pass-field">
            <span className="pass-label">{t("card.contract")}</span>
            <span className="pass-value">{offer.contract ?? missing}</span>
          </div>
        </div>
        <Stamp offer={offer} />
      </section>

      <div className="perforation" aria-hidden="true" />

      <section className="pass-body">
        {inTrash && <p className="pass-note">{t("card.inTrash")}</p>}

        <div className="pass-row">
          <StatusTag offer={offer} />
          <span className="pass-meta">
            {t("card.added")}: {fullDate(offer.addedAt, lang)}
          </span>
        </div>

        {!inTrash && (
          <>
            <div className="pass-controls">
              <label className="field field-inline">
                <span className="pass-label">{t("card.stage")}</span>
                <select value={offer.stage} onChange={(e) => onStage(e.target.value as Stage)}>
                  {STAGES.map((s) => (
                    <option key={s} value={s}>
                      {t(`stage.${s}`)}
                    </option>
                  ))}
                </select>
              </label>

              <fieldset className="interest-set">
                <legend className="pass-label">{t("interest.label")}</legend>
                {(["high", "maybe", "no"] as Interest[]).map((value, i) => (
                  <button
                    key={value}
                    type="button"
                    className="seg"
                    aria-pressed={offer.interest === value}
                    onClick={() => onInterest(offer.interest === value ? null : value)}
                  >
                    <kbd aria-hidden="true">{i + 1}</kbd> {t(`interest.${value}`)}
                  </button>
                ))}
                <button
                  type="button"
                  className="seg seg-icon"
                  aria-pressed={offer.favorite}
                  aria-label={offer.favorite ? t("favorite.remove") : t("favorite.add")}
                  title={offer.favorite ? t("favorite.remove") : t("favorite.add")}
                  onClick={() => onFavorite(!offer.favorite)}
                >
                  <IconStar filled={offer.favorite} />
                </button>
              </fieldset>
            </div>

            <div className="pass-actions">
              {isWebUrl(offer.url) && (
                <button type="button" className="btn btn-sign" onClick={() => onOpenLink(offer.url!)}>
                  <IconArrowOut /> {t("card.open")}
                </button>
              )}
              <button type="button" className="btn btn-ghost" onClick={onTrash}>
                <IconTrash /> {t("card.trash")}
              </button>
              {isWebUrl(offer.url) && (
                <button type="button" className="btn btn-ghost" onClick={onFullText} title={t("full.hint")}>
                  {t("full.button")}
                </button>
              )}
            </div>
          </>
        )}

        {inTrash && (
          <div className="pass-actions">
            <button type="button" className="btn btn-sign" onClick={onRestore}>
              <IconRestore /> {t("card.restore")}
            </button>
            <button type="button" className="btn btn-danger" onClick={onPurge}>
              <IconTrash /> {t("card.purge")}
            </button>
          </div>
        )}

        <AiPart offer={offer} aiOn={aiOn && !inTrash} onRateAgain={onRateAgain} onAssist={onAssist} />

        <FilterResults filters={offer.filters} />

        <dl className="pass-details">
          <div>
            <dt>{t("card.level")}</dt>
            <dd>{offer.level ?? missing}</dd>
          </div>
          <div>
            <dt>{t("card.language")}</dt>
            <dd>{offer.language?.toUpperCase() ?? missing}</dd>
          </div>
          <div>
            <dt>{t("card.sources")}</dt>
            <dd>
              {offer.links.map((l, i) => (
                <span key={l.source}>
                  {i > 0 && " · "}
                  {isWebUrl(l.url) ? (
                    <a
                      href={l.url}
                      onClick={(e) => {
                        e.preventDefault();
                        onOpenLink(l.url!);
                      }}
                    >
                      {sourceLabel(l.source, t)}
                    </a>
                  ) : (
                    sourceLabel(l.source, t)
                  )}
                </span>
              ))}
              {/* Adzuna's terms: offers from their API carry their name. */}
              {offer.sources.includes("adzuna") && <span className="adzuna-credit">{t("adzuna.credit")}</span>}
            </dd>
          </div>
        </dl>

        {!inTrash && (
          <label className="field">
            <span className="pass-label">{t("card.notes")}</span>
            <textarea
              rows={3}
              value={notes}
              maxLength={20000}
              aria-describedby="notes-hint"
              onChange={(e) => setNotes(e.target.value)}
              onBlur={() => notes !== offer.notes && onNotes(notes)}
            />
            <span id="notes-hint" className="hint">
              {t("card.notesHint")}
            </span>
          </label>
        )}

        {offer.requirements && (
          <section className="pass-text">
            <h3 className="pass-label">{t("card.requirements")}</h3>
            <p>{offer.requirements}</p>
          </section>
        )}
        {offer.description && (
          <section className="pass-text">
            <h3 className="pass-label">{t("card.description")}</h3>
            <p>{offer.description}</p>
          </section>
        )}

        <section className="pass-text">
          <h3 className="pass-label">{t("card.history")}</h3>
          <ol className="history">
            {offer.history.map((h, i) => (
              <li key={i}>
                <span className="mono">{fullDate(h.at, lang)}</span> {t(`stage.${h.stage}`)}
              </li>
            ))}
          </ol>
        </section>
      </section>
    </aside>
  );
});

const VERDICT_MARK = { pass: "✓", unsure: "?", fail: "✕" } as const;

/** Hard filter result for every profile, with the reasons in plain words. */
function FilterResults({ filters }: { filters: ProfileFilter[] }) {
  const { t } = useI18n();
  return (
    <section className="pass-text">
      <h3 className="pass-label">{t("filter.title")}</h3>
      {filters.length === 0 ? (
        <p className="pass-meta">{t("filter.noProfiles")}</p>
      ) : (
        <ul className="verdicts">
          {filters.map((f) => {
            const reasons = sortedReasons(f.reasons);
            return (
              <li key={f.profileId}>
                <span className="verdict-head">
                  <span className="verdict-profile">{f.profileName}</span>
                  <span className={`verdict verdict-${f.result}`}>
                    <span aria-hidden="true">{VERDICT_MARK[f.result]}</span> {t(`filter.result.${f.result}`)}
                  </span>
                </span>
                {reasons.length === 0 ? (
                  <span className="verdict-why">{t("filter.allFine")}</span>
                ) : (
                  <ul className="verdict-why">
                    {reasons.map((r, i) => (
                      <li key={i} className={`why-${r.kind}`}>
                        {reasonText(r, t)}
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

function Stamp({ offer }: { offer: OfferDetail }) {
  const { t } = useI18n();
  if (offer.score === null) {
    return (
      <div className="stamp stamp-none">
        <span className="stamp-label">{t("card.match")}</span>
        <span className="stamp-sub">{t("card.matchNone")}</span>
      </div>
    );
  }
  return (
    <div className={offer.scoreSource === "keywords" ? "stamp stamp-plain" : "stamp"}>
      <span className="stamp-label">{t("card.match")}</span>
      <span className="stamp-score">{offer.score}</span>
      <span className="stamp-sub">{offer.scoreSource === "ai" ? t("card.matchAi") : t("card.matchKeywords")}</span>
    </div>
  );
}

/** What AI read: summary, warnings, score for every profile with a quote, buttons for text on a click. */
function AiPart({
  offer,
  aiOn,
  onRateAgain,
  onAssist,
}: {
  offer: OfferDetail;
  aiOn: boolean;
  onRateAgain: () => void;
  onAssist: (kind: AssistKind) => void;
}) {
  const { t } = useI18n();
  const ai = offer.ai;
  const aiScores = ai.scores.filter((s) => s.source === "ai");
  return (
    <section className="pass-text ai-part" aria-labelledby="ai-title">
      <h3 id="ai-title" className="pass-label">
        {t("ai.cardTitle")}
      </h3>
      {ai.status === null && <p className="pass-meta">{aiOn ? t("ai.waiting") : t("ai.off")}</p>}
      {ai.status === "failed" && (
        <p className="source-warn">
          <IconWarning /> {t("ai.failed")}
        </p>
      )}
      {ai.summary && <p>{ai.summary}</p>}
      {ai.flags.length > 0 && (
        <div className="flags">
          <h4 className="flags-title">
            <IconWarning /> {t("ai.flags")}
          </h4>
          <ul>
            {ai.flags.map((f, i) => (
              <li key={i}>{f}</li>
            ))}
          </ul>
        </div>
      )}
      {ai.remoteQuote && (
        <p className="ai-quote-line">
          <span className="pass-label">{t("remote.label")}:</span> <q>{ai.remoteQuote}</q>
        </p>
      )}
      {aiScores.length > 0 && (
        <ul className="ai-scores">
          {aiScores.map((s) => (
            <li key={s.profileId}>
              <span className="verdict-head">
                <span className="verdict-profile">{s.profileName}</span>
                <span className="ai-score">{s.score}</span>
              </span>
              {s.why && <span className="verdict-why">{s.why}</span>}
              {s.quote && <q className="ai-quote">{s.quote}</q>}
            </li>
          ))}
        </ul>
      )}
      {aiOn && (
        <div className="ai-row">
          <button type="button" className="btn btn-ghost btn-small" onClick={() => onAssist("message")}>
            {t("assist.message")}
          </button>
          <button type="button" className="btn btn-ghost btn-small" onClick={() => onAssist("questions")}>
            {t("assist.questions")}
          </button>
          <button type="button" className="btn btn-ghost btn-small" onClick={() => onAssist("gaps")}>
            {t("assist.gaps")}
          </button>
          <button type="button" className="btn btn-ghost btn-small" onClick={onRateAgain}>
            {ai.status === null ? t("ai.rateThis") : t("ai.rateAgain")}
          </button>
        </div>
      )}
    </section>
  );
}
