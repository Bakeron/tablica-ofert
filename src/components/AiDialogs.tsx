import { useEffect, useId, useState } from "react";
import type { CareerIdea, Criteria } from "../api/types";
import { useI18n } from "../i18n";
import { droppedWords, wordDiff } from "../lib/diff";
import { IconArrowRight, IconWarning } from "./Icons";
import { CloseButton, Modal } from "./Modal";

/**
 * "Tidy up": the user's text and the AI proposal side by side, with what AI removed or changed
 * marked (struck out / underlined - not colour alone). The user accepts, edits or rejects.
 */
export function TidyDialog({
  open,
  before,
  proposal,
  maxLength,
  onAccept,
  onClose,
}: {
  open: boolean;
  before: string;
  proposal: string;
  /** The longest text the profile field takes - AI may write more. */
  maxLength: number;
  onAccept: (text: string) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [text, setText] = useState(proposal);
  useEffect(() => setText(proposal), [proposal]);
  const parts = wordDiff(before, text);
  const dropped = droppedWords(before, text);
  // Counted in characters, as Rust counts them.
  const over = [...text.trim()].length - maxLength;
  return (
    <Modal open={open} onClose={onClose} labelledBy="tidy-title" className="modal-wide">
      <div className="sheet">
        <header className="sheet-head">
          <h2 id="tidy-title">{t("tidy.title")}</h2>
          <CloseButton onClick={onClose} label={t("add.cancel")} />
        </header>
        <p className="hint">{t("tidy.intro")}</p>
        <div className="tidy-grid">
          <section>
            <h3 className="pass-label">{t("tidy.before")}</h3>
            <p className="tidy-text">
              {parts
                .filter((p) => p.kind !== "added")
                .map((p, i) =>
                  p.kind === "removed" ? (
                    <del key={i} title={t("tidy.removed")}>
                      {p.text}
                    </del>
                  ) : (
                    <span key={i}>{p.text}</span>
                  ),
                )}
            </p>
          </section>
          <section>
            <h3 className="pass-label">{t("tidy.after")}</h3>
            <p className="tidy-text" aria-hidden="true">
              {parts
                .filter((p) => p.kind !== "removed")
                .map((p, i) =>
                  p.kind === "added" ? (
                    <ins key={i} title={t("tidy.added")}>
                      {p.text}
                    </ins>
                  ) : (
                    <span key={i}>{p.text}</span>
                  ),
                )}
            </p>
          </section>
        </div>
        {dropped.length > 0 && (
          <div className="warn-box" role="note">
            <IconWarning />
            <p>
              <strong>{t("mail.warnTitle")}: </strong>
              {t("tidy.dropped", { words: dropped.slice(0, 20).join(", ") })}
            </p>
          </div>
        )}
        <label className="field">
          <span>{t("tidy.edit")}</span>
          <textarea rows={8} value={text} onChange={(e) => setText(e.target.value)} aria-invalid={over > 0 || undefined} />
          {over > 0 && (
            <span className="field-error" role="alert">
              <IconWarning /> {t("tidy.tooLong", { max: maxLength, over })}
            </span>
          )}
        </label>
        <footer className="sheet-foot">
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            {t("tidy.reject")}
          </button>
          <button type="button" className="btn btn-sign" disabled={!text.trim() || over > 0} onClick={() => onAccept(text.trim())}>
            {t("tidy.accept")}
          </button>
        </footer>
      </div>
    </Modal>
  );
}

/** Text written by AI on a click - to read, edit and copy. The app never sends it anywhere. */
export function AssistDialog({
  open,
  title,
  text,
  busy,
  onClose,
  onCopied,
}: {
  open: boolean;
  title: string;
  text: string;
  busy: boolean;
  onClose: () => void;
  onCopied: (ok: boolean) => void;
}) {
  const { t } = useI18n();
  const [value, setValue] = useState(text);
  useEffect(() => setValue(text), [text]);
  return (
    <Modal open={open} onClose={onClose} labelledBy="assist-title" className="modal-wide">
      <div className="sheet">
        <header className="sheet-head">
          <h2 id="assist-title">{title}</h2>
          <CloseButton onClick={onClose} label={t("settings.close")} />
        </header>
        {busy ? (
          <p role="status">{t("assist.busy")}</p>
        ) : (
          <>
            <p className="hint">{t("assist.note")}</p>
            <label className="field">
              <span className="sr-only">{title}</span>
              <textarea rows={14} value={value} onChange={(e) => setValue(e.target.value)} />
            </label>
          </>
        )}
        <footer className="sheet-foot">
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            {t("settings.close")}
          </button>
          <button
            type="button"
            className="btn btn-sign"
            disabled={busy || !value.trim()}
            onClick={() => navigator.clipboard.writeText(value).then(() => onCopied(true), () => onCopied(false))}
          >
            {t("mail.copy")}
          </button>
        </footer>
      </div>
    </Modal>
  );
}

/** One proposed filter value with its group - the user ticks what to add. */
export interface Suggestion {
  group: "keywords" | "workModes" | "cities" | "contracts" | "levels" | "languages" | "excludeWords";
  value: string;
  label: string;
}

export function suggestionsFrom(c: Criteria, label: (group: Suggestion["group"], value: string) => string): Suggestion[] {
  const out: Suggestion[] = [];
  const add = (group: Suggestion["group"], values: string[]) => values.forEach((value) => out.push({ group, value, label: label(group, value) }));
  add("keywords", c.keywords);
  add("workModes", c.workModes);
  add("cities", c.cities.map((x) => x.name));
  add("contracts", c.contracts);
  add("levels", c.levels);
  add("languages", c.languages);
  add("excludeWords", c.excludeWords);
  return out;
}

/** Proposed filters, each with a tick box; values already in the form are shown as already there. */
export function SuggestPanel({
  items,
  present,
  onAdd,
  onClose,
}: {
  items: Suggestion[];
  /** Is this value already in the form? */
  present: (s: Suggestion) => boolean;
  onAdd: (chosen: Suggestion[]) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const fresh = items.filter((s) => !present(s));
  const [chosen, setChosen] = useState<Set<number>>(() => new Set(fresh.map((_, i) => i)));
  if (items.length === 0) {
    return (
      <div className="suggest" role="status">
        <p>{t("suggest.none")}</p>
        <button type="button" className="btn btn-ghost btn-small" onClick={onClose}>
          {t("settings.close")}
        </button>
      </div>
    );
  }
  return (
    <div className="suggest" role="group" aria-label={t("suggest.title")}>
      <p>
        <strong>{t("suggest.title")}</strong> {t("suggest.intro")}
      </p>
      <ul className="suggest-list">
        {fresh.map((s, i) => (
          <li key={`${s.group}-${s.value}`}>
            <label className="choice-item">
              <input
                type="checkbox"
                checked={chosen.has(i)}
                onChange={() =>
                  setChosen((c) => {
                    const next = new Set(c);
                    if (next.has(i)) next.delete(i);
                    else next.add(i);
                    return next;
                  })
                }
              />
              <span>{s.label}</span>
            </label>
          </li>
        ))}
      </ul>
      {fresh.length < items.length && <p className="hint">{t("suggest.already", { count: items.length - fresh.length })}</p>}
      <div className="source-row">
        <button type="button" className="btn btn-ghost btn-small" onClick={onClose}>
          {t("suggest.skip")}
        </button>
        <button type="button" className="btn btn-sign btn-small" disabled={chosen.size === 0} onClick={() => onAdd(fresh.filter((_, i) => chosen.has(i)))}>
          {t("suggest.add")}
        </button>
      </div>
    </div>
  );
}

/**
 * Jobs the AI proposes. The reason opens on a click (a phone has no hover); each name goes to
 * the keywords only by its own button.
 */
export function CareersPanel({
  ideas,
  present,
  onAdd,
  onClose,
}: {
  ideas: CareerIdea[];
  /** Is this name already among the keywords? */
  present: (name: string) => boolean;
  onAdd: (name: string) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const id = useId();
  const [open, setOpen] = useState<Set<number>>(new Set());
  const toggle = (i: number) =>
    setOpen((o) => {
      const next = new Set(o);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      return next;
    });
  return (
    <div className="suggest" role="group" aria-label={t("careers.title")}>
      <p>
        <strong>{t("careers.title")}</strong> {ideas.length > 0 ? t("careers.intro") : t("careers.none")}
      </p>
      {ideas.length > 0 && (
        <ul className="career-list">
          {ideas.map((idea, i) => {
            const added = present(idea.name);
            return (
              <li key={idea.name} className="career">
                <div className="career-head">
                  <button type="button" className="source-unfold career-name" aria-expanded={open.has(i)} aria-controls={`${id}-${i}`} onClick={() => toggle(i)}>
                    <IconArrowRight />
                    {idea.name}
                    <span className="source-unfold-hint">{open.has(i) ? t("careers.hideWhy") : t("careers.showWhy")}</span>
                  </button>
                  <button type="button" className="btn btn-ghost btn-small" disabled={added} onClick={() => onAdd(idea.name)}>
                    {added ? t("careers.added") : t("careers.add")}
                  </button>
                </div>
                <div id={`${id}-${i}`} className="career-why" hidden={!open.has(i)}>
                  <p>{idea.why}</p>
                </div>
              </li>
            );
          })}
        </ul>
      )}
      <div className="source-row">
        <button type="button" className="btn btn-ghost btn-small" onClick={onClose}>
          {t("settings.close")}
        </button>
      </div>
    </div>
  );
}
