import { useEffect, useRef, useState, type FormEvent } from "react";
import type { Captured, CheckedField, Draft, DupVerdict, FieldNote, LowFunds, OfferInput, Remote, SalaryPeriod, SameField, SameOffer } from "../api/types";
import { lowFundsLine } from "../lib/aiProblem";
import { formDirty } from "../lib/formDirty";
import { IconWarning } from "./Icons";
import { useI18n, type TextKey } from "../i18n";
import { isPhone } from "../lib/platform";
import { CloseButton, Modal } from "./Modal";
import { Reveal } from "./Reveal";
import { SameOfferPanel } from "./SameOfferPanel";
import { FieldError, FieldErrorsProvider, useFieldErrors } from "./FieldError";
import { OFFER_LONG_MAX, OFFER_SHORT_MAX, OFFER_URL_MAX } from "../lib/limits";

const EMPTY = {
  title: "",
  company: "",
  url: "",
  city: "",
  remote: "unknown" as Remote,
  salaryMin: "",
  salaryMax: "",
  salaryCurrency: "PLN",
  salaryPeriod: "month" as SalaryPeriod,
  contract: "",
  level: "",
  /** Days (YYYY-MM-DD) for the date fields. */
  published: "",
  expires: "",
  description: "",
};

const pad = (n: number) => String(n).padStart(2, "0");

/** The day of a date in the local time, as a date field shows it. */
function dayOf(iso: string | null): string {
  const d = iso ? new Date(iso) : null;
  return d && !Number.isNaN(d.getTime()) ? `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` : "";
}

/** A day of a date field as the app stores it: published - its midnight, open until - the end
 *  of that day in Poland (the same hours as `dates::on_day`). The exact date read from the text
 *  ("2 hours ago") stays while the day is not changed. */
function dateOf(day: string, exact: string | null, end: boolean): string | null {
  if (day === "") return null;
  if (exact && dayOf(exact) === day) return exact;
  return `${day}T${end ? "21:59:59" : "00:00:00"}Z`;
}

function toNumber(value: string): number | null {
  const digits = value.replace(/[\s ]/g, "").replace(",", ".");
  if (digits === "") return null;
  const n = Math.round(Number(digits));
  return Number.isFinite(n) ? n : null;
}

const orNull = (value: string) => (value.trim() === "" ? null : value.trim());

/** The checked field a form field belongs to - typing in it takes its "check" note away. */
const CHECKED: Partial<Record<keyof typeof EMPTY, CheckedField>> = {
  title: "title",
  company: "company",
  city: "city",
  remote: "remote",
  salaryMin: "salary",
  salaryMax: "salary",
  salaryCurrency: "salary",
  salaryPeriod: "salary",
  contract: "contract",
  level: "level",
  published: "published",
  expires: "expires",
};

const checkId = (field: CheckedField) => `check-${field}`;

/** "check" and why (the value is not in the text, the text has more...) and the line of the
 *  offer the value comes from - under the field. */
function Check({ note }: { note: FieldNote | undefined }) {
  const { t } = useI18n();
  if (!note) return null;
  const why =
    note.problem === "notInText"
      ? t("check.notInText")
      : note.problem === "missing"
        ? t(`check.missing.${note.field}` as TextKey)
        : note.problem === "more"
          ? t("check.more", { more: note.more ?? "" })
          : null;
  return (
    <span id={checkId(note.field)} className="field-check">
      {why && (
        <span className="check-why">
          <b className="check-mark">{t("check.mark")}</b> {why}
        </span>
      )}
      {note.fragment && (
        <span className="check-src">
          {t("check.source")} <q>{note.fragment}</q>
        </span>
      )}
    </span>
  );
}

/** How to copy an offer's text from a browser: Chrome first, the other browsers folded. */
function CopyHelp() {
  const { t } = useI18n();
  return (
    <details className="howto">
      <summary>{t("quick.howTitle")}</summary>
      <p className="hint">{t("quick.howPreview")}</p>
      <ol className="steps-list">
        {[1, 2, 3].map((i) => (
          <li key={i}>{t(`quick.howChrome${i}` as TextKey)}</li>
        ))}
      </ol>
      <details className="howto">
        <summary>{t("quick.howOther")}</summary>
        <ul className="steps-list">
          {(["Firefox", "Edge", "Opera"] as const).map((b) => (
            <li key={b}>{t(`quick.how${b}` as TextKey)}</li>
          ))}
        </ul>
      </details>
    </details>
  );
}

export function AddOfferDialog({
  open,
  shared,
  aiOn,
  onClose,
  onSave,
  onCheck,
  onShow,
  onUpdate,
  onCompare,
  onJudge,
  onOpenPreview,
  onCapture,
  onFill,
}: {
  open: boolean;
  /** Text shared from the phone's browser ("Share" → Tablica Ofert). */
  shared?: { id: number; text: string } | null;
  /** An AI provider is set - "Fill in the fields" uses AI. */
  aiOn: boolean;
  onClose: () => void;
  /** `reading` - scores AI gave while filling the fields (stored with the offer); `place` - where
   *  the person said it goes (a separate offer, a child of an offer). */
  onSave: (offer: OfferInput, reading: string | null, place?: { separate: true } | { parent: number }) => Promise<boolean>;
  /** Before saving: the same offer of the same portal already on the board? `null` = it failed. */
  onCheck: (offer: OfferInput) => Promise<SameOffer | null>;
  /** "Show it": the offer on the board (`own` = move it to "Own" first). */
  onShow: (id: number, own: boolean) => Promise<boolean>;
  /** "Update the existing one" / "Fill in" with the form's fields, without the ones in `keep`. */
  onUpdate: (id: number, offer: OfferInput, reading: string | null, keep: SameField[], renew: boolean) => Promise<boolean>;
  /** "Fill in that one": what the form would change in offer `id`. */
  onCompare: (id: number, offer: OfferInput) => Promise<SameOffer | null>;
  /** AI's word about "probably the same" offers. */
  onJudge: (offer: OfferInput, ids: number[]) => Promise<DupVerdict[] | null>;
  onOpenPreview: (url: string) => Promise<boolean>;
  onCapture: () => Promise<Captured | null>;
  onFill: (text: string, title: string | null) => Promise<Draft | null>;
}) {
  const { t, lang } = useI18n();
  const { errors, formProps, field } = useFieldErrors(open);
  const [form, setForm] = useState(EMPTY);
  const [busy, setBusy] = useState(false);
  const [link, setLink] = useState("");
  const [pasted, setPasted] = useState("");
  const [pageTitle, setPageTitle] = useState<string | null>(null);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [working, setWorking] = useState<"capture" | "fill" | null>(null);
  const [filledBy, setFilledBy] = useState<"ai" | "code" | null>(null);
  const [readingId, setReadingId] = useState<string | null>(null);
  /** The dates exactly as read from the text (the fields hold only days). */
  const [exact, setExact] = useState<{ published: string | null; expires: string | null }>({ published: null, expires: null });
  const [notes, setNotes] = useState<FieldNote[]>([]);
  // Claude read the offer and little of its top-up is left.
  const [lowFunds, setLowFunds] = useState<LowFunds | null>(null);
  /** The same offer found on the board, with the form as it would be saved; `back` - the list of
   *  "probably the same" offers "Fill in that one" came from. */
  const [found, setFound] = useState<{ same: SameOffer; offer: OfferInput; back?: SameOffer } | null>(null);
  const judged = useRef(new Map<string, DupVerdict[]>());
  const note = (field: CheckedField) => notes.find((n) => n.field === field);
  /** Props of a form field with a note: the link to it and the yellow edge for "check". */
  const checked = (field: CheckedField) => {
    const n = note(field);
    return n ? { "aria-describedby": checkId(field), "data-check": n.problem ? "problem" : "source" } : {};
  };

  // Closed and opened again: no old question under the form.
  useEffect(() => {
    if (!open) setFound(null);
  }, [open]);

  // Shared from the phone: the link goes to the link field, the rest is the offer text.
  useEffect(() => {
    if (!open || !shared) return;
    const url = shared.text.match(/https?:\/\/\S+/)?.[0] ?? "";
    const rest = shared.text.replace(url, "").trim();
    setLink(url);
    setForm((f) => ({ ...f, url: url || f.url }));
    if (rest) {
      setPasted(rest);
      fill(rest, null, url);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, shared?.id]);

  async function openPreview() {
    if (await onOpenPreview(link.trim())) setPreviewOpen(true);
  }

  async function capture() {
    setWorking("capture");
    const page = await onCapture();
    setWorking(null);
    if (!page) return;
    setPasted(page.text);
    setPageTitle(page.title);
    setLink(page.url);
    await fill(page.text, page.title, page.url);
  }

  async function fill(text = pasted, title = pageTitle, url = link) {
    if (!text.trim()) return;
    setWorking("fill");
    const draft = await onFill(text, title);
    setWorking(null);
    if (!draft) return;
    const v = (x: string | number | null | undefined) => (x === null || x === undefined ? "" : String(x));
    setForm((f) => ({
      ...f,
      title: draft.title ?? f.title,
      company: draft.company ?? f.company,
      url: url.trim() || f.url,
      city: draft.city ?? f.city,
      remote: draft.remote ?? f.remote,
      salaryMin: draft.salaryMin !== null ? v(draft.salaryMin) : f.salaryMin,
      salaryMax: draft.salaryMax !== null ? v(draft.salaryMax) : f.salaryMax,
      salaryCurrency: draft.salaryCurrency ?? f.salaryCurrency,
      salaryPeriod: draft.salaryPeriod ?? f.salaryPeriod,
      contract: draft.contract ?? f.contract,
      level: draft.level ?? f.level,
      published: draft.publishedAt ? dayOf(draft.publishedAt) : f.published,
      expires: draft.expiresAt ? dayOf(draft.expiresAt) : f.expires,
      description: draft.description ?? f.description,
    }));
    setExact((x) => ({ published: draft.publishedAt ?? x.published, expires: draft.expiresAt ?? x.expires }));
    setFilledBy(draft.byAi ? "ai" : "code");
    setReadingId(draft.readingId);
    setNotes(draft.notes ?? []);
    setLowFunds(draft.lowFunds ?? null);
  }

  const set = <K extends keyof typeof EMPTY>(key: K, value: (typeof EMPTY)[K]) => {
    setForm((f) => ({ ...f, [key]: value }));
    // The question was about the form as it was.
    setFound(null);
    // Changed by hand - the note was about AI's value (or the empty field).
    const field = CHECKED[key];
    if (field) setNotes((now) => now.filter((n) => n.field !== field));
  };

  /** The form as it is saved. */
  function formOffer(): OfferInput {
    return {
      title: form.title,
      company: form.company,
      source: "manual",
      url: orNull(form.url),
      city: orNull(form.city),
      remote: form.remote,
      salaryMin: toNumber(form.salaryMin),
      salaryMax: toNumber(form.salaryMax),
      salaryCurrency: orNull(form.salaryCurrency),
      salaryPeriod: form.salaryPeriod,
      contract: orNull(form.contract),
      level: orNull(form.level),
      description: orNull(form.description),
      publishedAt: dateOf(form.published, exact.published, false),
      expiresAt: dateOf(form.expires, exact.expires, true),
    };
  }

  /** Everything back to an empty form, the dialog closed. */
  function done() {
    setFound(null);
    setReadingId(null);
    setExact({ published: null, expires: null });
    setForm(EMPTY);
    setLink("");
    setPasted("");
    setPageTitle(null);
    setFilledBy(null);
    setNotes([]);
    onClose();
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    const offer = formOffer();
    setBusy(true);
    // The same offer of the same portal may already be there - the person decides what then.
    const same = await onCheck(offer);
    if (same && same.kind !== "none") {
      setBusy(false);
      setFound({ same, offer });
      return;
    }
    await save(offer);
  }

  async function save(offer: OfferInput, place?: { separate: true } | { parent: number }) {
    setBusy(true);
    const ok = await onSave(offer, readingId, place);
    setBusy(false);
    if (ok) done();
  }

  async function update(id: number, offer: OfferInput, keep: SameField[], renew: boolean) {
    setBusy(true);
    const ok = await onUpdate(id, offer, readingId, keep, renew);
    setBusy(false);
    if (ok) done();
  }

  /** AI's word, once per form and list - back from "Fill in that one" it is not asked (and paid) again. */
  async function judge(offer: OfferInput, ids: number[]): Promise<DupVerdict[] | null> {
    const key = JSON.stringify([offer, ids]);
    const known = judged.current.get(key);
    if (known) return known;
    const verdicts = await onJudge(offer, ids);
    if (verdicts) judged.current.set(key, verdicts);
    return verdicts;
  }

  /** "Fill in that one": the table of what changes there, with the way back to the list. */
  async function complete(id: number, offer: OfferInput, back: SameOffer) {
    setBusy(true);
    const same = await onCompare(id, offer);
    setBusy(false);
    if (same) setFound({ same, offer, back });
  }

  async function show(id: number, own: boolean) {
    if (await onShow(id, own)) done();
  }

  return (
    <Modal
      open={open}
      onClose={onClose}
      labelledBy="add-title"
      className="modal-wide"
      // Esc or the phone's "back" with something typed or shared: a question first, nothing lost.
      dirty={formDirty(form, EMPTY, [link, pasted], busy)}
    >
      <FieldErrorsProvider value={errors}>
      <form onSubmit={submit} className="sheet" autoComplete="off" {...formProps}>
        <header className="sheet-head">
          <h2 id="add-title">{t("add.title")}</h2>
          <CloseButton onClick={onClose} label={t("add.cancel")} />
        </header>
        <p className="hint">{t("add.intro")}</p>

        <fieldset className="quick">
          <legend>{t(isPhone() ? "quick.titlePhone" : "quick.title")}</legend>
          {/* The phone has no preview window - "Share" from the browser does this job there,
              and the link goes straight to the form's own link field. */}
          {!isPhone() && (
            <div className="quick-row">
              <label className="field">
                <span>{t("quick.link")}</span>
                <input type="url" maxLength={OFFER_URL_MAX} placeholder="https://" value={link} onChange={(e) => setLink(e.target.value)} />
              </label>
              <button type="button" className="btn btn-ghost btn-small" disabled={!/^https?:\/\/.+/i.test(link.trim())} onClick={openPreview}>
                {t("quick.open")}
              </button>
            </div>
          )}
          <Reveal show={previewOpen}>
            <div className="quick-row quick-capture">
              <p className="hint">{t("quick.captureHint")}</p>
              <button type="button" className="btn btn-sign btn-small" disabled={working !== null} aria-busy={working === "capture"} onClick={capture}>
                {working === "capture" ? t("quick.capturing") : t("quick.capture")}
              </button>
            </div>
          </Reveal>
          <label className="field">
            <span>{t(isPhone() ? "quick.pastePhone" : "quick.paste")}</span>
            <textarea rows={3} maxLength={OFFER_LONG_MAX} value={pasted} onChange={(e) => setPasted(e.target.value)} />
          </label>
          <div className="quick-row">
            <span className="hint" role="status">
              {filledBy === "ai" ? t("quick.byAi") : filledBy === "code" ? t("quick.byCode") : ""}
            </span>
            <button type="button" className="btn btn-ghost btn-small" disabled={!pasted.trim() || working !== null} aria-busy={working === "fill"} onClick={() => fill()}>
              {working === "fill" ? t("ai.working") : aiOn ? t("quick.fillAi") : t("quick.fill")}
            </button>
          </div>
          {lowFunds && (
            <p className="source-warn" role="status">
              <IconWarning /> {lowFundsLine(lowFunds, t, lang)} {t("ai.lowFundsWhere")}
            </p>
          )}
          {!isPhone() && <CopyHelp />}
        </fieldset>

        <div className="form-grid">
          <label className="field span-2">
            <span>
              {t("add.fieldTitle")} <em className="req">({t("add.required")})</em>
            </span>
            <input
              required
              maxLength={OFFER_SHORT_MAX}
              value={form.title}
              onChange={(e) => set("title", e.target.value)}
              data-autofocus={!isPhone() || undefined}
              data-check={checked("title")["data-check"]}
              {...field("offer-title", note("title") && checkId("title"))}
            />
            <FieldError name="offer-title" />
            <Check note={note("title")} />
          </label>
          <label className="field span-2">
            <span>
              {t("add.fieldCompany")} <em className="req">({t("add.required")})</em>
            </span>
            <input
              required
              maxLength={OFFER_SHORT_MAX}
              value={form.company}
              onChange={(e) => set("company", e.target.value)}
              data-check={checked("company")["data-check"]}
              {...field("offer-company", note("company") && checkId("company"))}
            />
            <FieldError name="offer-company" />
            <Check note={note("company")} />
          </label>
          <label className="field span-2">
            <span>{t("add.fieldUrl")}</span>
            <input
              type="url"
              pattern="https?://.+"
              maxLength={OFFER_URL_MAX}
              placeholder="https://"
              value={form.url}
              onChange={(e) => set("url", e.target.value)}
              data-error={t("form.url")}
              {...field("offer-url")}
            />
            <FieldError name="offer-url" />
          </label>
          <label className="field">
            <span>{t("add.fieldCity")}</span>
            <input maxLength={OFFER_SHORT_MAX} value={form.city} onChange={(e) => set("city", e.target.value)} {...checked("city")} />
            <Check note={note("city")} />
          </label>
          <label className="field">
            <span>{t("add.fieldRemote")}</span>
            <select value={form.remote} onChange={(e) => set("remote", e.target.value as Remote)} {...checked("remote")}>
              {(["unknown", "yes", "hybrid", "no"] as Remote[]).map((r) => (
                <option key={r} value={r}>
                  {t(`remote.${r}`)}
                </option>
              ))}
            </select>
            <Check note={note("remote")} />
          </label>
          <label className="field">
            <span>{t("add.fieldSalaryMin")}</span>
            <input inputMode="numeric" value={form.salaryMin} onChange={(e) => set("salaryMin", e.target.value)} {...checked("salary")} />
          </label>
          <label className="field">
            <span>{t("add.fieldSalaryMax")}</span>
            <input inputMode="numeric" value={form.salaryMax} onChange={(e) => set("salaryMax", e.target.value)} {...checked("salary")} />
          </label>
          {note("salary") && (
            <div className="check-row">
              <Check note={note("salary")} />
            </div>
          )}
          <label className="field">
            <span>{t("add.fieldCurrency")}</span>
            <input
              maxLength={3}
              autoCapitalize="characters"
              pattern="[A-Za-z]{3}"
              value={form.salaryCurrency}
              onChange={(e) => set("salaryCurrency", e.target.value.toUpperCase())}
              data-error={t("form.currency")}
              {...field("offer-currency")}
            />
            <FieldError name="offer-currency" />
          </label>
          <label className="field">
            <span>{t("add.fieldPeriod")}</span>
            <select value={form.salaryPeriod} onChange={(e) => set("salaryPeriod", e.target.value as SalaryPeriod)}>
              <option value="month">{t("add.periodMonth")}</option>
              <option value="hour">{t("add.periodHour")}</option>
            </select>
          </label>
          <label className="field">
            <span>{t("add.fieldContract")}</span>
            <input maxLength={OFFER_SHORT_MAX} list="contracts" value={form.contract} onChange={(e) => set("contract", e.target.value)} {...checked("contract")} />
            <datalist id="contracts">
              <option value="UoP" />
              <option value="B2B" />
              <option value={t("contract.mandate")} />
            </datalist>
            <Check note={note("contract")} />
          </label>
          <label className="field">
            <span>{t("add.fieldLevel")}</span>
            <input maxLength={OFFER_SHORT_MAX} list="levels" value={form.level} onChange={(e) => set("level", e.target.value)} {...checked("level")} />
            <datalist id="levels">
              <option value="junior" />
              <option value="mid" />
              <option value="senior" />
            </datalist>
            <Check note={note("level")} />
          </label>
          <label className="field">
            <span>{t("add.fieldPublished")}</span>
            <input type="date" value={form.published} onChange={(e) => set("published", e.target.value)} {...checked("published")} />
            <Check note={note("published")} />
          </label>
          <label className="field">
            <span>{t("add.fieldExpires")}</span>
            <input type="date" value={form.expires} onChange={(e) => set("expires", e.target.value)} {...checked("expires")} />
            <Check note={note("expires")} />
          </label>
          <label className="field span-2">
            <span>{t("add.fieldDescription")}</span>
            <textarea
              rows={6}
              maxLength={OFFER_LONG_MAX}
              aria-describedby="desc-hint"
              value={form.description}
              onChange={(e) => set("description", e.target.value)}
            />
            <span id="desc-hint" className="hint">
              {t("add.descriptionHint")}
            </span>
          </label>
        </div>

        <footer className="sheet-foot">
          {found?.same.was ? (
            <SameOfferPanel
              found={found.same}
              offer={found.offer}
              busy={busy}
              aiOn={aiOn}
              onShow={() => show(found.same.was!.id, found.same.kind === "identical" && !found.same.was!.own)}
              onUpdate={(keep, renew) => update(found.same.was!.id, found.offer, keep, renew)}
              onSave={(place) => save(found.offer, place)}
              onComplete={(id) => complete(id, found.offer, found.same)}
              onJudge={(ids) => judge(found.offer, ids)}
              onBack={() => setFound(found.back ? { same: found.back, offer: found.offer } : null)}
              backToList={Boolean(found.back)}
            />
          ) : (
            <>
              <button type="button" className="btn btn-ghost" onClick={onClose}>
                {t("add.cancel")}
              </button>
              <button type="submit" className="btn btn-sign" disabled={busy}>
                {t("add.save")}
              </button>
            </>
          )}
        </footer>
      </form>
      </FieldErrorsProvider>
    </Modal>
  );
}
