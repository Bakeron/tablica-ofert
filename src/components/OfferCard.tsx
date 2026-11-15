import { forwardRef, useEffect, useRef, useState, type ReactNode } from "react";
import { STAGES, type AssistKind, type DupCandidate, type DupVerdict, type Interest, type OfferDetail, type ProfileFilter, type Stage } from "../api/types";
import { plural, useI18n } from "../i18n";
import { companyLabel, fullDate, fullDay, isWebUrl, linksByCity, placeLabel, salaryFull, shownScore, signedPoints, translatedTitle } from "../lib/format";
import { useAppearOn } from "../lib/presence";
import { LEARN_MAX_POINTS, LOW_KEYWORD_SCORE, OFFER_NOTES_MAX } from "../lib/limits";
import { isPhone } from "../lib/platform";
import { reasonText, sortedReasons } from "../lib/profile";
import { isNarrowLayout } from "../lib/fit";
import { isSwipe, lockAxis, type SwipeAxis } from "../lib/swipe";
import { sourceLabel } from "./Board";
import { IconArrowLeft, IconArrowOut, IconRestore, IconStar, IconTrash, IconWarning } from "./Icons";
import { AgendaPart, type AgendaActions } from "./AgendaPart";
import { Help } from "./Help";
import { StatusTag } from "./StatusTag";
import { DupCandidates, type Verdicts } from "./DupCandidates";
import { CloseButton, Modal } from "./Modal";
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
  /** "Restore" in the bin - any kind (not for me, expired, deleted). */
  onRestore: () => void;
  onPurge: () => void;
  /** "Expired" (or "Still open" when it is marked). */
  onExpired: () => void;
  /** "To own" - shown for a collected offer only. */
  onOwn: () => void;
  /** The checkbox "Own offer": on / off for the whole family of ads. */
  onOwnChange: (own: boolean) => void;
  /** "Renewed after your CV - send it again?": `true` = sent once more. */
  onRenewed: (resent: boolean) => void;
  /** Another ad of the tree chosen on the card (a child, or the parent of a child). */
  onShowOffer: (id: number) => void;
  /** "Detach" - a child becomes an offer of its own. */
  onDetach: () => void;
  /** "The same company and title - the same ad?": `true` = join as a child, `false` = separate. */
  onDupAnswer: (join: boolean) => void;
  /** "Check duplicates": offers that may be the same job (code); `null` = it failed. */
  onFindDuplicates: () => Promise<DupCandidate[] | null>;
  /** AI's word about them; `null` = no answer. */
  onJudgeDuplicates: (ids: number[]) => Promise<DupVerdict[] | null>;
  /** "Join as a child" with offer `other` (the first added is the parent). */
  onJoin: (other: number) => void;
  /** What the offer's own page said about being over, when it was taken a moment ago. */
  expiredPage: string | null;
  onKeepExpiredPage: () => void;
  /** A swipe on the card where it is a screen of its own: right = "very", left = "no", then the next offer. */
  onSwipe: (interest: Interest) => void;
  /** An AI provider is set. */
  aiOn: boolean;
  /** Claude is turned on - "Rate more precisely" for a collected offer. */
  claudeOn: boolean;
  onRateAgain: () => void;
  onRatePrecise: () => void;
  /** "Translate" the description of an offer in another language. */
  onTranslate: () => Promise<void>;
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
  { offer, onStage, onInterest, onFavorite, onNotes, onOpenLink, onTrash, onRestore, onPurge, onExpired, onOwn, onOwnChange, onRenewed, onShowOffer, onDetach, onDupAnswer, onFindDuplicates, onJudgeDuplicates, onJoin, expiredPage, onKeepExpiredPage, onSwipe, aiOn, claudeOn, onRateAgain, onRatePrecise, onTranslate, onAssist, onOpenAiSettings, onFullText, onBackToList, agenda, showDates, showContacts },
  headingRef,
) {
  const { t, lang } = useI18n();
  // The note stays with its offer: a swipe to the next offer saves it there, not in the next one.
  const [notes, setNotes] = useState({ id: offer?.id, text: offer?.notes ?? "", saved: offer?.notes ?? "" });

  useEffect(() => setNotes({ id: offer?.id, text: offer?.notes ?? "", saved: offer?.notes ?? "" }), [offer?.id, offer?.notes]);
  // An offer in another language shows the AI translation; the original is one click away.
  const [showOriginal, setShowOriginal] = useState(false);
  useEffect(() => setShowOriginal(false), [offer?.id]);
  const [translating, setTranslating] = useState(false);
  /** "Check duplicates": the offers found and AI's word; `null` = not checked. */
  const [dups, setDups] = useState<{ id: number; list: DupCandidate[] | "wait"; verdicts: Verdicts } | null>(null);
  useEffect(() => setDups(null), [offer?.id]);
  /** "Description" opened in its window. */
  const [descriptionOpen, setDescriptionOpen] = useState(false);
  useEffect(() => setDescriptionOpen(false), [offer?.id]);
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

  /** "Check duplicates": the code's list at once, AI's word when it comes (only for this offer). */
  async function checkDuplicates() {
    if (!offer) return;
    const id = offer.id;
    setDups({ id, list: "wait", verdicts: null });
    const list = await onFindDuplicates();
    const ask = Boolean(list && list.length > 0 && aiOn);
    setDups((d) => (d?.id !== id ? d : list ? { id, list, verdicts: ask ? "wait" : null } : null));
    if (!list || !ask) return;
    const verdicts = await onJudgeDuplicates(list.map((c) => c.was.id));
    setDups((d) => (d?.id === id ? { ...d, verdicts: verdicts ?? "error" } : d));
  }

  // "Check duplicates": code finds, AI (when set) says what it thinks, the person decides.
  // The button stands beside "Rate this offer"; what it found comes right under that row.
  const canCheckDups = offer.parentId === null && !inTrash;
  const dupButton = canCheckDups && (dups === null || dups.list === "wait") && (
    <button type="button" className="btn btn-ghost btn-small" onClick={checkDuplicates} title={t("dup.checkHint")} disabled={dups !== null} aria-busy={dups !== null}>
      {t("dup.check")}
    </button>
  );
  const dupResult = canCheckDups && dups !== null && (
    <div className="dup-check">
      {dups.list === "wait" ? (
        <p className="hint" role="status">
          {t("dup.looking")}
        </p>
      ) : dups.list.length === 0 ? (
        <p className="hint" role="status">
          {t("dup.none")}
        </p>
      ) : (
        <>
          <h4 className="pass-label">{t("dup.found")}</h4>
          <DupCandidates
            candidates={dups.list}
            verdicts={dups.verdicts}
            actions={(c) => (
              <>
                <button type="button" className="btn btn-ghost btn-small" onClick={() => onJoin(c.was.id)}>
                  {t("dup.join")}
                </button>
                <button type="button" className="btn btn-ghost btn-small" onClick={() => onShowOffer(c.was.id)}>
                  {t("same.showThat")}
                </button>
              </>
            )}
          />
          <p className="hint">{t("dup.youDecide")}</p>
        </>
      )}
    </div>
  );

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
          {hasTranslation && (
            <p className="pass-meta pass-translation">
              {t(!translated ? "card.original" : descriptionTr ? "card.translated" : "card.translatedTitle")}{" "}
              <button type="button" className="btn btn-ghost btn-small" onClick={() => setShowOriginal(!showOriginal)}>
                {t(translated ? "card.showOriginal" : "card.showTranslation")}
              </button>
            </p>
          )}
          {/* A rating translates only the title; the description costs a second answer as long as the
              offer, so it is translated on a click. */}
          {foreign && !descriptionTr && !inTrash && (
            <p className="pass-meta pass-translation">
              {aiOn ? (
                <>
                  {t("card.foreign", { language: t(`language.${foreign}`) })}{" "}
                  <button
                    type="button"
                    className="btn btn-ghost btn-small"
                    disabled={translating}
                    aria-busy={translating}
                    onClick={() => {
                      setTranslating(true);
                      onTranslate().finally(() => setTranslating(false));
                    }}
                  >
                    {t(translating ? "card.translating" : "card.translate")}
                  </button>
                </>
              ) : (
                t(isPhone() ? "card.translateNoAiPhone" : "card.translateNoAi", { language: t(`language.${foreign}`) })
              )}
            </p>
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
        {/* In the bin: why it is there, as a mark and in a sentence. */}
        {offer.bin && (
          <p className="pass-note">
            <span className={`row-mark row-mark-bin row-mark-${offer.bin}`}>{t(`row.bin.${offer.bin}`)}</span> {t(`card.bin.${offer.bin}`)}
          </p>
        )}

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
            {offer.expiresAt && (
              <>
                {" · "}
                {t("card.expires")}: {fullDay(offer.expiresAt, lang)}
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

        {/* The tree: a child says whose ad it is and can leave; a parent lists its other ads. */}
        {offer.parentId !== null && !inTrash && (
          <div className="pass-proposal tree-note">
            <p>{t("tree.childNote")}</p>
            <div className="pass-actions">
              <button type="button" className="btn btn-ghost btn-small" onClick={() => onShowOffer(offer.parentId!)}>
                {t("tree.showParent")}
              </button>
              <button type="button" className="btn btn-ghost btn-small" onClick={onDetach} title={t("tree.detachHint")}>
                {t("tree.detach")}
              </button>
            </div>
          </div>
        )}
        {offer.children.length > 0 && (
          <section className="pass-text">
            <h3 className="pass-label">{t(plural(lang, "tree.ads", offer.children.length + 1), { count: offer.children.length + 1 })}</h3>
            {/* Every ad has its own stage; the CV sent through a child counts for the whole job. */}
            {offer.sentVia && (
              <p className="same-applied">
                {t("tree.sentViaCard", { source: sourceLabel(offer.sentVia.source, t), date: fullDay(offer.sentVia.at, lang) })}
              </p>
            )}
            <ol className="tree-ads">
              <li>
                <span className="tree-ad-main">{t("tree.first")}</span> {offer.sources.map((s) => sourceLabel(s, t)).join(" · ")}
                {" · "}
                <span className="mono">{fullDay(offer.addedAt, lang)}</span>
                {" · "}
                <span className="tree-ad-stage">{t(`stage.${offer.stage}`)}</span>
              </li>
              {offer.children.map((c) => (
                <li key={c.id}>
                  <button type="button" className="btn btn-ghost btn-small" onClick={() => onShowOffer(c.id)}>
                    {c.sources.map((s) => sourceLabel(s, t)).join(" · ")}
                    {c.city && c.city !== offer.city ? ` - ${c.city}` : ""}
                  </button>{" "}
                  <span className="mono">{fullDay(c.addedAt, lang)}</span>
                  {" · "}
                  <span className="tree-ad-stage">{t(`stage.${c.stage}`)}</span>
                </li>
              ))}
            </ol>
          </section>
        )}

        {/* The same company and title in another city / with other salary: never joined by itself. */}
        {offer.dup && !inTrash && (
          <div className="pass-proposal dup-note">
            <p>
              <span className="row-mark row-mark-dup">{t("row.dup")}</span>{" "}
              {t(offer.dup.why === "salary" ? "dup.card.salary" : "dup.card.city", {
                title: offer.dup.was.title,
                sources: offer.dup.sources.map((s) => sourceLabel(s, t)).join(" · "),
                was: offer.dup.why === "salary" ? salaryFull(offer.dup.was, lang, t) : (offer.dup.was.city ?? missing),
                now: offer.dup.why === "salary" ? salaryFull(offer, lang, t) : (offer.city ?? missing),
              })}
            </p>
            <div className="pass-actions">
              <button type="button" className="btn btn-ghost btn-small" onClick={() => onDupAnswer(true)}>
                {t("dup.join")}
              </button>
              <button type="button" className="btn btn-ghost btn-small" onClick={() => onShowOffer(offer.dup!.was.id)}>
                {t("same.showThat")}
              </button>
              <button type="button" className="btn btn-ghost btn-small" onClick={() => onDupAnswer(false)}>
                {t("dup.separate")}
              </button>
            </div>
          </div>
        )}

        {/* The ad came again after the CV went: the decision is the person's - the app sends nothing. */}
        {offer.renewAsked && !inTrash && (
          <div className="pass-proposal">
            <p>
              <span className="row-mark row-mark-renewed">{t("row.renewed")}</span>{" "}
              {t("card.renewedAsk", { date: offer.renewedAt ? fullDay(offer.renewedAt, lang) : "" })}
            </p>
            <div className="pass-actions">
              <button type="button" className="btn btn-sign btn-small" onClick={() => onRenewed(true)}>
                {t("card.renewedResent")}
              </button>
              <button type="button" className="btn btn-ghost btn-small" onClick={() => onRenewed(false)}>
                {t("card.renewedSkip")}
              </button>
            </div>
          </div>
        )}

        {offer.bin && (
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
        <AiPart
          offer={offer}
          aiOn={aiOn && !inTrash}
          aiOff={!aiOn && !inTrash && !isPhone()}
          precise={claudeOn && !offer.manual}
          onRateAgain={onRateAgain}
          onRatePrecise={onRatePrecise}
          onAssist={onAssist}
          onOpenAiSettings={onOpenAiSettings}
          dupButton={dupButton || null}
          dupResult={dupResult || null}
        />

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
        {/* The description is a wall of text read rarely - a window on a click, not a long scroll. */}
        {(offer.description || (!inTrash && isWebUrl(offer.url) && !isPhone())) && (
          <div className="pass-actions">
            {offer.description && (
              <button type="button" className="btn btn-ghost" onClick={() => setDescriptionOpen(true)} aria-haspopup="dialog">
                {t("card.description")}
              </button>
            )}
            {!inTrash && isWebUrl(offer.url) && !isPhone() && (
              <button type="button" className="btn btn-ghost" onClick={onFullText} title={t("full.hint")}>
                {t("full.button")}
              </button>
            )}
          </div>
        )}
        <Modal open={descriptionOpen && Boolean(offer.description)} onClose={() => setDescriptionOpen(false)} labelledBy="description-title" className="modal-wide">
          <div className="sheet">
            <header className="sheet-head">
              <h2 id="description-title">{t("card.description")}</h2>
              <CloseButton onClick={() => setDescriptionOpen(false)} />
            </header>
            <p className="description-text" lang={translated && descriptionTr ? lang : (foreign ?? undefined)}>
              {/* Ads often leave whole runs of empty lines - one is enough. */}
              {(translated && descriptionTr ? descriptionTr : (offer.description ?? "")).replace(/[ \t]+\n/g, "\n").replace(/\n{3,}/g, "\n\n").trim()}
            </p>
            <footer className="sheet-foot">
              <button type="button" className="btn btn-ghost" onClick={() => setDescriptionOpen(false)}>
                {t("settings.close")}
              </button>
            </footer>
          </div>
        </Modal>

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
            {/* Own or collected - one for the whole job (every ad of the family); added by hand = own. */}
            <label className="choice-item own-check">
              <input type="checkbox" checked={offer.own} onChange={(e) => onOwnChange(e.target.checked)} />
              <span>
                {t("own.check")}
                {(offer.parentId !== null || offer.children.length > 0) && <span className="hint"> {t("own.checkFamily")}</span>}
              </span>
            </label>

            <AgendaPart key={offer.id} agenda={offer.agenda} actions={agenda} showDates={showDates} showContacts={showContacts} />

            <label className="field">
              <span className="pass-label">{t("card.notes")}</span>
              <textarea
                rows={3}
                value={notes.id === offer.id ? notes.text : offer.notes}
                maxLength={OFFER_NOTES_MAX}
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
            {[
              ...offer.history.map((h) => ({ at: h.at, text: t(`stage.${h.stage}`) })),
              ...(offer.renewedAt ? [{ at: offer.renewedAt, text: t("card.historyRenewed") }] : []),
            ]
              .sort((a, b) => a.at.localeCompare(b.at))
              .map((h, i) => (
                <li key={i}>
                  <span className="mono">{fullDate(h.at, lang)}</span> {h.text}
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
          {!offer.own && (
            <button type="button" className="btn btn-ghost pass-own" onClick={onOwn} title={t("own.hint")}>
              {!isPhone() && <kbd aria-hidden="true">D</kbd>} {t("own.button")}
            </button>
          )}
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
  precise,
  onRateAgain,
  onRatePrecise,
  onAssist,
  onOpenAiSettings,
  dupButton,
  dupResult,
}: {
  offer: OfferDetail;
  aiOn: boolean;
  /** No provider is on and the place to turn one on is at hand. */
  aiOff: boolean;
  /** "Rate more precisely" with Claude - a collected offer (one added by hand goes to Claude anyway). */
  precise: boolean;
  onRateAgain: () => void;
  onRatePrecise: () => void;
  onAssist: (kind: AssistKind) => void;
  onOpenAiSettings: () => void;
  /** "Check duplicates" - beside "Rate this offer" (also without AI: code finds them). */
  dupButton: ReactNode;
  /** What "Check duplicates" found, under that row. */
  dupResult: ReactNode;
}) {
  const { t } = useI18n();
  const ai = offer.ai;
  const aiScores = ai.scores.filter((s) => s.source === "ai");
  return (
    <section className="pass-text ai-part" aria-labelledby="ai-title">
      <h3 id="ai-title" className="pass-label">
        {t("ai.cardTitle")}
        {ai.precise && (
          <span className="row-mark ai-precise-mark" title={t("ai.preciseMarkHint")}>
            {t("ai.preciseMark")}
          </span>
        )}
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
      {/* Checking the offer first (rate, duplicates), then texts written for the person: two in a row, one whole. */}
      {(aiOn || dupButton) && (
        // Alone in its row (duplicates checked, or no AI) - a button takes the whole width.
        <div className={`ai-grid${aiOn && dupButton ? "" : " ai-grid-one"}`}>
          {aiOn && (
            <button type="button" className="btn btn-ghost btn-small" onClick={onRateAgain}>
              {ai.status === null ? t("ai.rateThis") : t("ai.rateAgain")}
            </button>
          )}
          {dupButton}
          {aiOn && precise && (
            <button type="button" className="btn btn-ghost btn-small ai-grid-wide" onClick={onRatePrecise}>
              {t("ai.ratePrecise")}
            </button>
          )}
        </div>
      )}
      {dupResult}
      {aiOn && (
        <div className="ai-grid ai-grid-write">
          <button type="button" className="btn btn-ghost btn-small" onClick={() => onAssist("questions")}>
            {t("assist.questions")}
          </button>
          <button type="button" className="btn btn-ghost btn-small" onClick={() => onAssist("gaps")}>
            {t("assist.gaps")}
          </button>
          <button type="button" className="btn btn-ghost btn-small ai-grid-wide" onClick={() => onAssist("message")}>
            {t("assist.message")}
          </button>
        </div>
      )}
    </section>
  );
}
