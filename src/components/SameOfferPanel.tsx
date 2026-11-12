import { useEffect, useRef, useState } from "react";
import type { DupVerdict, OfferInput, SameField, SameOffer, SameWas } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { companyLabel, fullDay, salaryFull } from "../lib/format";
import { sourceLabel } from "./Board";
import { DupCandidates, type Verdicts } from "./DupCandidates";

/** What a field of the offer on the board (`was`) or of the form (`now`) says, in words. */
function valueOf(field: SameField, o: SameWas | OfferInput, lang: "pl" | "en", t: (key: TextKey, vars?: Record<string, string | number>) => string): string {
  const missing = t("card.missing");
  switch (field) {
    case "title":
      return o.title;
    case "company":
      return companyLabel(o.company, t);
    case "city":
      return o.city ?? missing;
    case "remote":
      return t(`remote.${o.remote ?? "unknown"}`);
    case "salary":
      return salaryFull(
        { salaryMin: o.salaryMin ?? null, salaryMax: o.salaryMax ?? null, salaryCurrency: o.salaryCurrency ?? null, salaryPeriod: o.salaryPeriod ?? null },
        lang,
        t,
      );
    case "contract":
      return o.contract ?? missing;
    case "level":
      return o.level ?? missing;
    case "published":
      return o.publishedAt ? fullDay(o.publishedAt, lang) : missing;
    case "expires":
      return o.expiresAt ? fullDay(o.expiresAt, lang) : missing;
    case "description":
      return "";
  }
}

/** "Add offer" found the same (or probably the same) offer on the board: what differs and what to
 *  do. Nothing is merged by itself - every way out is a button. Where both versions of a field
 *  differ, the person picks one (the new one is ticked). */
export function SameOfferPanel({
  found,
  offer,
  busy,
  aiOn,
  onShow,
  onUpdate,
  onSave,
  onComplete,
  onJudge,
  onBack,
  backToList = false,
}: {
  found: SameOffer;
  /** The form as it would be saved. */
  offer: OfferInput;
  busy: boolean;
  /** An AI provider is set - "probably the same" offers are judged by AI. */
  aiOn: boolean;
  onShow: () => void;
  /** Writes the form into `found.was` - without the fields in `keep`; `renew` = the ad came again. */
  onUpdate: (keep: SameField[], renew: boolean) => void;
  /** Saves the form: as the rules say (`undefined`), as a separate offer, or as a child of `parent`. */
  onSave: (place?: { separate: true } | { parent: number }) => void;
  /** "Fill in that one" - what the form would change in offer `id`. */
  onComplete: (id: number) => void;
  /** AI's word about the "probably the same" offers; `null` = no answer. */
  onJudge: (ids: number[]) => Promise<DupVerdict[] | null>;
  onBack: () => void;
  /** "Back" goes to the list of "probably the same" offers, not to the form. */
  backToList?: boolean;
}) {
  const { t, lang } = useI18n();
  const heading = useRef<HTMLHeadingElement>(null);
  /** Fields where the person keeps the old version. */
  const [keep, setKeep] = useState<Set<SameField>>(new Set());
  const [verdicts, setVerdicts] = useState<Verdicts>(null);
  // The question replaces the buttons under the form - the eye and the screen reader go there.
  useEffect(() => heading.current?.focus(), [found]);
  useEffect(() => setKeep(new Set()), [found]);
  // "Probably the same": AI judges the offers the code found (only here and on the card's click).
  useEffect(() => {
    if (found.kind !== "probable" || !aiOn || found.candidates.length === 0) {
      setVerdicts(null);
      return;
    }
    let live = true;
    setVerdicts("wait");
    onJudge(found.candidates.map((c) => c.was.id)).then((v) => live && setVerdicts(v ?? "error"));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [found, aiOn]);
  const was = found.was;
  if (!was) return null;
  const name = { title: was.title, company: companyLabel(was.company, t) };
  const rows = found.fields.filter((f) => f !== "description");
  const textChanged = found.fields.includes("description");
  const kind = found.kind;
  const appliedShown = Boolean(was.appliedAt) && kind !== "identical" && kind !== "second" && kind !== "probable" && kind !== "joined";
  const withTable = kind === "changed" || kind === "similar" || kind === "partial" || kind === "complete";
  const pick = (field: SameField, old: boolean) =>
    setKeep((now) => {
      const next = new Set(now);
      if (old) next.add(field);
      else next.delete(field);
      return next;
    });

  /** One cell of a field with two versions: a radio button and the value. */
  const choice = (field: SameField, old: boolean, text: string) => (
    <label className="same-pick">
      <input type="radio" name={`same-${field}`} checked={keep.has(field) === old} onChange={() => pick(field, old)} />
      <span>{text}</span>
    </label>
  );

  const text = (() => {
    switch (kind) {
      case "second":
        return t(was.expiresAt ? "same.second.text" : "same.second.textNoDate", { ...name, date: was.expiresAt ? fullDay(was.expiresAt, lang) : "" });
      case "similar":
        return t("same.similar.text", { ...name, similarity: found.similarity });
      case "ask":
        return t(found.fields.includes("salary") ? "same.ask.salaryText" : "same.ask.cityText", {
          ...name,
          was: valueOf(found.fields.includes("salary") ? "salary" : "city", was, lang, t),
          now: valueOf(found.fields.includes("salary") ? "salary" : "city", offer, lang, t),
        });
      case "complete":
        return t(rows.some((f) => !found.filled.includes(f)) || (textChanged && !found.filled.includes("description")) ? "same.complete.text" : "same.complete.textAllEmpty", name);
      default:
        return t(`same.${kind}.text` as TextKey, name);
    }
  })();
  const titleKey = kind === "ask" ? (found.fields.includes("salary") ? "same.ask.salaryTitle" : "same.ask.cityTitle") : `same.${kind}.title`;

  return (
    <div className="same-box" role="group" aria-labelledby="same-title">
      <h3 id="same-title" ref={heading} tabIndex={-1} className="same-title">
        {t(titleKey as TextKey, name)}
      </h3>
      <p>{text}</p>
      {withTable && (rows.length > 0 || textChanged) && (
        <table className="same-diff">
          <thead>
            <tr>
              <th scope="col">{t("same.col.field")}</th>
              <th scope="col">{t("same.col.was")}</th>
              <th scope="col">{t("same.col.now")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((f) => (
              <tr key={f}>
                <th scope="row">{t(`same.field.${f}` as TextKey)}</th>
                {found.filled.includes(f) ? (
                  <>
                    <td>{valueOf(f, was, lang, t)}</td>
                    <td>
                      {valueOf(f, offer, lang, t)} <span className="same-fill">{t("same.fills")}</span>
                    </td>
                  </>
                ) : (
                  <>
                    <td>{choice(f, true, valueOf(f, was, lang, t))}</td>
                    <td>{choice(f, false, valueOf(f, offer, lang, t))}</td>
                  </>
                )}
              </tr>
            ))}
            {textChanged && (
              <tr>
                <th scope="row">{t("same.field.description")}</th>
                {found.filled.includes("description") ? (
                  <td colSpan={2}>
                    {t("same.textFull")} <span className="same-fill">{t("same.fills")}</span>
                  </td>
                ) : (
                  <>
                    <td>{choice("description", true, t("same.textKeep"))}</td>
                    <td>{choice("description", false, t("same.textChanged", { similarity: found.similarity }))}</td>
                  </>
                )}
              </tr>
            )}
          </tbody>
        </table>
      )}
      {kind === "probable" && (
        <>
          <DupCandidates
            candidates={found.candidates}
            verdicts={verdicts}
            actions={(c) => (
              <>
                <button type="button" className="btn btn-ghost btn-small" disabled={busy} onClick={() => onSave({ parent: c.was.id })}>
                  {t("dup.join")}
                </button>
                <button type="button" className="btn btn-ghost btn-small" disabled={busy} onClick={() => onComplete(c.was.id)}>
                  {t("dup.fillThat")}
                </button>
              </>
            )}
          />
          <p className="hint">{t("dup.youDecide")}</p>
        </>
      )}
      {appliedShown && (
        <p className="same-applied">{t(kind === "ask" ? "same.appliedThat" : "same.applied", { date: fullDay(was.appliedAt!, lang) })}</p>
      )}
      {/* The CV already went for this job through some ad of its family - another one does not need it. */}
      {was.sentVia && !appliedShown && kind !== "probable" && (
        <p className="same-applied">{t("same.sentVia", { date: fullDay(was.sentVia.at, lang), source: sourceLabel(was.sentVia.source, t) })}</p>
      )}
      <div className="same-actions">
        {kind === "identical" && (
          <button type="button" className="btn btn-sign" onClick={onShow}>
            {t(was.own ? "same.show" : "same.showOwn")}
          </button>
        )}
        {kind === "changed" && (
          <>
            <button type="button" className="btn btn-sign" disabled={busy} onClick={() => onUpdate([...keep], true)}>
              {t("same.update")}
            </button>
            <button type="button" className="btn btn-ghost" onClick={onShow}>
              {t("same.showUnchanged")}
            </button>
          </>
        )}
        {(kind === "partial" || kind === "complete") && (
          <button type="button" className="btn btn-sign" disabled={busy} onClick={() => onUpdate([...keep], false)}>
            {t("same.fill")}
          </button>
        )}
        {kind === "similar" && (
          <>
            <button type="button" className="btn btn-sign" disabled={busy} onClick={() => onUpdate([...keep], true)}>
              {t("same.yesUpdate")}
            </button>
            <button type="button" className="btn btn-ghost" disabled={busy} onClick={() => onSave({ separate: true })}>
              {t("same.noSeparate")}
            </button>
          </>
        )}
        {kind === "second" && (
          <>
            <button type="button" className="btn btn-sign" disabled={busy} onClick={() => onSave()}>
              {t("same.saveSecond")}
            </button>
            <button type="button" className="btn btn-ghost" onClick={onShow}>
              {t("same.showThat")}
            </button>
          </>
        )}
        {kind === "joined" && (
          <>
            <button type="button" className="btn btn-sign" disabled={busy} onClick={() => onSave({ parent: was.id })}>
              {t("same.saveJoined")}
            </button>
            <button type="button" className="btn btn-ghost" onClick={onShow}>
              {t("same.showThat")}
            </button>
          </>
        )}
        {kind === "ask" && (
          <>
            <button type="button" className="btn btn-sign" disabled={busy} onClick={() => onSave({ separate: true })}>
              {t("dup.separate")}
            </button>
            <button type="button" className="btn btn-ghost" disabled={busy} onClick={() => onSave({ parent: was.id })}>
              {t("dup.join")}
            </button>
          </>
        )}
        {kind === "probable" && (
          <button type="button" className="btn btn-sign" disabled={busy} onClick={() => onSave({ separate: true })}>
            {t("dup.separate")}
          </button>
        )}
        <button type="button" className="btn btn-ghost" onClick={onBack}>
          {t(backToList ? "same.backList" : "same.back")}
        </button>
      </div>
    </div>
  );
}
