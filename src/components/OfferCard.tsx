import { forwardRef, useEffect, useRef, useState } from "react";
import { STAGES, type AssistKind, type Interest, type OfferDetail, type ProfileFilter, type Stage } from "../api/types";
import { useI18n } from "../i18n";
import { companyLabel, fullDate, isWebUrl, linksByCity, placeLabel, salaryFull, shownScore, signedPoints, translatedTitle } from "../lib/format";
import { useAppearOn } from "../lib/presence";
import { LEARN_MAX_POINTS, LOW_KEYWORD_SCORE } from "../lib/limits";
import { isPhone } from "../lib/platform";
import { reasonText, sortedReasons } from "../lib/profile";
import { isNarrowLayout } from "../lib/fit";
import { isSwipe, lockAxis, type SwipeAxis } from "../lib/swipe";
import { sourceLabel } from "./Board";
import { IconArrowLeft, IconArrowOut, IconRestore, IconStar, IconTrash, IconWarning } from "./Icons";
import { AgendaPart, type AgendaActions } from "./AgendaPart";
import { Help } from "./Help";
import { StatusTag } from "./StatusTag";
// Official logo from adzuna.com/press.html, scaled down (Adzuna's API terms ask for it next to their offers).
import adzunaLogo from "../assets/adzuna-logo.png";

const ADZUNA_URL = "https://www.adzuna.pl/";
const HIMALAYAS_URL = "https://himalayas.app/";

interface Props {
  offer: OfferDetail | null;
  onStage: (stage: Stage) => void;
  onInterest: (interest: Interest | null) => void;
  onFavorite: (favorite: boolean) => void;
  /** With the offer the note was written in - the card may already show the next one when it is saved. */
  onNotes: (id: number, notes: string) => void;
  onOpenLink: (url: string) => void;
  onTrash: () => void;
  onRestore: () => void;
  onPurge: () => void;
  /** "Expired" (or "Still open" when it is marked). */
  onExpired: () => void;
  /** What the offer's own page said about being over, when it was taken a moment ago. */
  expiredPage: string | null;
  onKeepExpiredPage: () => void;
  /** "Bring back to the list" in the archive. */
  onUnarchive: () => void;
  /** A swipe on the card where it is a screen of its own: right = "very", left = "no", then the next offer. */
  onSwipe: (interest: Interest) => void;
  /** An AI provider is set. */
  aiOn: boolean;
  onRateAgain: () => void;
  onAssist: (kind: AssistKind) => void;
  /** Settings at the AI section. */
  onOpenAiSettings: () => void;
  /** Opens the offer page in the preview window to take its full text. */
  onFullText: () => void;
  /** Narrow layout (the card is a screen of its own): back to the list. */
  onBackToList: () => void;
  agenda: AgendaActions;
  showDates: boolean;
  showContacts: boolean;
}

/** Offer details shaped like a boarding pass: key data on the stub above the perforation. */
export const OfferCard = forwardRef<HTMLHeadingElement, Props>(function OfferCard(
  { offer, onStage, onInterest, onFavorite, onNotes, onOpenLink, onTrash, onRestore, onPurge, onExpired, expiredPage, onKeepExpiredPage, onUnarchive, onSwipe, aiOn, onRateAgain, onAssist, onOpenAiSettings, onFullText, onBackToList, agenda, showDates, showContacts },
  headingRef,
) {
  const { t, lang } = useI18n();
  // The note stays with its offer: a swipe to the next offer saves it there, not in the next one.
  const [notes, setNotes] = useState({ id: offer?.id, text: offer?.notes ?? "", saved: offer?.notes ?? "" });

  useEffect(() => setNotes({ id: offer?.id, text: offer?.notes ?? "", saved: offer?.notes ?? "" }), [offer?.id, offer?.notes]);
  // An offer in another language shows the AI translation; the original is one click away.
  const [showOriginal, setShowOriginal] = useState(false);
  useEffect(() => setShowOriginal(false), [offer?.id]);
  /** Where a finger started on the card (a swipe = the next offer). */
  const touch = useRef<{ x: number; y: number; axis: SwipeAxis | null } | null>(null);
  // Another offer chosen - the card changes with a short fade, not a jump.
  const passRef = useRef<HTMLElement>(null);
  useAppearOn(passRef, offer?.id ?? null);

  if (!offer) {
    return (
      <aside className="pass pass-empty" aria-label={t("card.title")}>
        <p>{t("card.empty")}</p>
      </aside>
    );
  }

  const inTrash = offer.deletedAt !== null;
  const inArchive = !inTrash && offer.archivedAt !== null;
  const missing = t("card.missing");
  const cityLinks = linksByCity(offer.links);
  const titleTr = translatedTitle(offer, lang);
  const descriptionTr = offer.translationLang === lang ? offer.ai.descriptionTranslated : null;
  const hasTranslation = Boolean(titleTr || descriptionTr);
  const translated = hasTranslation && !showOriginal;
  const foreign = offer.ai.offerLanguage !== null && offer.ai.offerLanguage !== lang ? offer.ai.offerLanguage : null;
  const remoteText = t(`remote.${offer.remote}`);

  return (
    <aside
      className="pass"
      aria-labelledby="pass-title"
      ref={passRef}
      onTouchStart={(e) => (touch.current = { x: e.touches[0].clientX, y: e.touches[0].clientY, axis: null })}
      onTouchMove={(e) => {
        const start = touch.current;
        if (start) start.axis ??= lockAxis(e.touches[0].clientX - start.x, e.touches[0].clientY - start.y);
      }}
      onTouchEnd={(e) => {
        const start = touch.current;
        touch.current = null;
        // Only where the card is a screen of its own, and never inside a field or a sideways strip.
        if (!start || start.axis !== "x" || inTrash || inArchive || !isNarrowLayout()) return;
        if ((e.target as Element).closest("input, textarea, select, .seg-strip, .interest-set")) return;
        const dx = e.changedTouches[0].clientX - start.x;
        if (isSwipe(dx, e.changedTouches[0].clientY - start.y)) onSwipe(dx > 0 ? "high" : "no");
      }}
      onTouchCancel={() => (touch.current = null)}
    >
      {/* Only where the card is a screen of its own (CSS shows it there). */}
      <div className="pass-back-bar">
        <button type="button" className="btn btn-ghost btn-small" onClick={onBackToList}>
          <IconArrowLeft /> {t("card.backToList")}
        </button>
        {/* The swipe is done on the list, so its tip stands by the way back to it. */}
        {isPhone() && <Help text={t("interest.swipeHelp")} />}
      </div>
      {/* Stub - the part you would tear off */}
      <section className="pass-stub">
        <div className="pass-field pass-field-wide">
          <span className="pass-label">{t("card.title")}</span>
          <h2 id="pass-title" ref={headingRef} tabIndex={-1} className="pass-title" lang={translated && titleTr ? lang : (foreign ?? undefined)}>
            {translated && titleTr ? titleTr : offer.title}
          </h2>
          {hasTranslation ? (
            <p className="pass-meta pass-translation">
              {t(translated ? "card.translated" : "card.original")}{" "}
              <button type="button" className="btn btn-ghost btn-small" onClick={() => setShowOriginal(!showOriginal)}>
                {t(translated ? "card.showOriginal" : "card.showTranslation")}
              </button>
            </p>
          ) : (
            foreign &&
            !inTrash && (
              <p className="pass-meta pass-translation">
                {t(!aiOn ? (isPhone() ? "card.translateNoAiPhone" : "card.translateNoAi") : offer.ai.status === "done" ? "card.translateAgain" : "card.translateWaiting", {
                  language: t(`language.${foreign}`),
                })}
              </p>
            )
          )}
        </div>
        <div className="pass-field pass-field-wide">
          <span className="pass-label">{t("card.company")}</span>
          <span className="pass-company">{companyLabel(offer.company, t)}</span>
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

        {inArchive && <p className="pass-note">{t("archive.note")}</p>}

        <div className="pass-row">
          <StatusTag offer={offer} />
          <span className="pass-meta">
            {t("card.published")}: {fullDate(offer.publishedAt, lang)}
            {/* Added later than published (a fetch, a mail) - both dates say something. */}
            {offer.addedAt.slice(0, 16) !== offer.publishedAt.slice(0, 16) && (
              <>
                {" · "}
                {t("card.added")}: {fullDate(offer.addedAt, lang)}
              </>
            )}
          </span>
        </div>

        {offer.score !== null && offer.learned !== 0 && !inTrash && (
          <p className="pass-meta pass-learned">
            {t("card.learned", { points: signedPoints(offer.learned), words: offer.learnedWords.join(", ") })}{" "}
            <Help text={t(isPhone() ? "card.learnedHelpPhone" : "card.learnedHelp", { max: LEARN_MAX_POINTS })} />
          </p>
        )}

        {/* Only a proposal: the page may lie or be about something else, so nothing happens by itself. */}
        {expiredPage && !offer.expired && !inTrash && !inArchive && (
          <div className="pass-proposal">
            <p>
              <IconWarning /> {t("expired.pageSays", { phrase: expiredPage })}
            </p>
            <div className="pass-actions">
              <button type="button" className="btn btn-sign btn-small" onClick={onExpired}>
                {t("expired.pageMark")}
              </button>
              <button type="button" className="btn btn-ghost btn-small" onClick={onKeepExpiredPage}>
                {t("expired.pageKeep")}
              </button>
            </div>
          </div>
        )}

        {inArchive && (
          <div className="pass-actions">
            <button type="button" className="btn btn-sign" onClick={onUnarchive}>
              <IconRestore /> {t("archive.restore")}
            </button>
          </div>
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

        {/* What one comes to the card for goes first: AI's reading, then the offer's own text. */}
        <AiPart offer={offer} aiOn={aiOn && !inTrash} aiOff={!aiOn && !inTrash && !isPhone()} onRateAgain={onRateAgain} onAssist={onAssist} onOpenAiSettings={onOpenAiSettings} />

        {/* The job closed on a source whose terms ask to drop it - only the user's own record is left. */}
        {offer.contentRemoved === "himalayas" && (
          <section className="pass-text">
            <h3 className="pass-label">{t("card.description")}</h3>
            <p>{t("card.contentRemoved.himalayas")}</p>
          </section>
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
            <p lang={translated && descriptionTr ? lang : (foreign ?? undefined)}>
              {translated && descriptionTr ? descriptionTr : offer.description}
            </p>
          </section>
        )}
        {!inTrash && isWebUrl(offer.url) && !isPhone() && (
          <div className="pass-actions">
            <button type="button" className="btn btn-ghost" onClick={onFullText} title={t("full.hint")}>
              {t("full.button")}
            </button>
          </div>
        )}

        {!inTrash && (
          <>
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

            <AgendaPart key={offer.id} agenda={offer.agenda} actions={agenda} showDates={showDates} showContacts={showContacts} />

            <label className="field">
              <span className="pass-label">{t("card.notes")}</span>
              <textarea
                rows={3}
                value={notes.id === offer.id ? notes.text : offer.notes}
                maxLength={20000}
                aria-describedby="notes-hint"
                onChange={(e) => {
                  const text = e.target.value;
                  setNotes((n) => (n.id === offer.id ? { ...n, text } : { id: offer.id, text, saved: offer.notes }));
                }}
                onBlur={() => notes.id !== undefined && notes.text !== notes.saved && onNotes(notes.id, notes.text)}
              />
              <span id="notes-hint" className="hint">
                {t("card.notesHint")}
              </span>
            </label>
          </>
        )}

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
              {offer.links.map((l, i) => {
                const name = cityLinks.length > 1 && l.city ? `${sourceLabel(l.source, t)} - ${l.city}` : sourceLabel(l.source, t);
                return (
                  <span key={`${l.source}|${l.url ?? ""}|${i}`}>
                    {i > 0 && " · "}
                    {isWebUrl(l.url) ? (
                      <a
                        href={l.url}
                        onClick={(e) => {
                          e.preventDefault();
                          onOpenLink(l.url!);
                        }}
                      >
                        {name}
                      </a>
                    ) : (
                      name
                    )}
                  </span>
                );
              })}
              {/* Adzuna's API terms: their logo, at least 116 x 23 px, linked to their site. */}
              {offer.sources.includes("adzuna") && (
                <a
                  className="adzuna-credit"
                  href={ADZUNA_URL}
                  onClick={(e) => {
                    e.preventDefault();
                    onOpenLink(ADZUNA_URL);
                  }}
                >
                  <span>{t("adzuna.credit")}</span>
                  <img src={adzunaLogo} width={120} height={31} alt="Adzuna" />
                </a>
              )}
              {/* Himalayas' API terms: visible credit with a link to their site. */}
              {offer.sources.includes("himalayas") && (
                <a
                  className="himalayas-credit"
                  href={HIMALAYAS_URL}
                  onClick={(e) => {
                    e.preventDefault();
                    onOpenLink(HIMALAYAS_URL);
                  }}
                >
                  {t("himalayas.credit")}
                </a>
              )}
            </dd>
          </div>
        </dl>

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

        {/* Far from the main button, so a slip of the thumb never throws the offer away. */}
        {!inTrash && (
          <div className="pass-actions">
            <button type="button" className="btn btn-ghost" onClick={onTrash}>
              <IconTrash /> {t("card.trash")}
            </button>
          </div>
        )}
      </section>

      {/* The everyday decisions stay under the thumb all the way down the card, like the stub of a pass. */}
      {!inTrash && !inArchive && (
        <div className="pass-dock">
          <fieldset className="interest-set">
            <legend className="sr-only">{t("interest.label")}</legend>
            {(["high", "maybe", "no"] as Interest[]).map((value, i) => (
              <button
                key={value}
                type="button"
                className="seg"
                aria-pressed={offer.interest === value}
                onClick={() => onInterest(offer.interest === value ? null : value)}
              >
                {!isPhone() && <kbd aria-hidden="true">{i + 1}</kbd>} {t(`interest.${value}`)}
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
          <button type="button" className="btn btn-ghost pass-expired" onClick={onExpired} title={offer.expired ? undefined : t("expired.hint")}>
            {!isPhone() && !offer.expired && <kbd aria-hidden="true">W</kbd>} {t(offer.expired ? "expired.undoButton" : "expired.button")}
          </button>
          {cityLinks.length > 1
            ? cityLinks.map((l) => (
                <button key={l.url} type="button" className="btn btn-sign pass-open" onClick={() => onOpenLink(l.url)}>
                  <IconArrowOut /> {t("card.openIn", { city: l.city })}
                </button>
              ))
            : isWebUrl(offer.url) && (
                <button type="button" className="btn btn-sign pass-open" onClick={() => onOpenLink(offer.url!)}>
                  <IconArrowOut /> {t("card.open")}
                </button>
              )}
        </div>
      )}
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
  if (offer.scoreSource === "keywords") {
    return (
      <div className="stamp stamp-plain">
        <span className="stamp-label">{t("card.matchWords")}</span>
        <span className="stamp-score">{shownScore(offer)}</span>
        <span className="stamp-sub">
          {t("card.matchOf100")} <Help text={t(isPhone() ? "card.matchWordsHelpPhone" : "card.matchWordsHelp", { score: LOW_KEYWORD_SCORE })} />
        </span>
      </div>
    );
  }
  return (
    <div className="stamp">
      <span className="stamp-label">{t("card.match")}</span>
      <span className="stamp-score">{shownScore(offer)}</span>
      <span className="stamp-sub">{t("card.matchAi")}</span>
    </div>
  );
}

/** What AI read: summary, warnings, score for every profile with a quote, buttons for text on a click. */
function AiPart({
  offer,
  aiOn,
  aiOff,
  onRateAgain,
  onAssist,
  onOpenAiSettings,
}: {
  offer: OfferDetail;
  aiOn: boolean;
  /** No provider is on and the place to turn one on is at hand. */
  aiOff: boolean;
  onRateAgain: () => void;
  onAssist: (kind: AssistKind) => void;
  onOpenAiSettings: () => void;
}) {
  const { t } = useI18n();
  const ai = offer.ai;
  const aiScores = ai.scores.filter((s) => s.source === "ai");
  return (
    <section className="pass-text ai-part" aria-labelledby="ai-title">
      <h3 id="ai-title" className="pass-label">
        {t("ai.cardTitle")}
      </h3>
      {ai.status === null && <p className="pass-meta">{aiOn ? t("ai.waiting") : t(isPhone() ? "ai.offPhone" : "ai.off")}</p>}
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
      {aiOff && (
        <>
          {ai.status !== null && <p className="pass-meta">{t("ai.offAgain")}</p>}
          <div className="ai-row">
            <button type="button" className="btn btn-ghost btn-small" onClick={onOpenAiSettings}>
              {t("ai.openSettings")}
            </button>
          </div>
        </>
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
