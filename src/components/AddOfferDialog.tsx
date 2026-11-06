import { useEffect, useState, type FormEvent } from "react";
import type { Captured, Draft, OfferInput, Remote, SalaryPeriod } from "../api/types";
import { useI18n } from "../i18n";
import { isPhone } from "../lib/platform";
import { CloseButton, Modal } from "./Modal";
import { Reveal } from "./Reveal";
import { FieldError, FieldErrorsProvider, useFieldErrors } from "./FieldError";

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
  description: "",
};

function toNumber(value: string): number | null {
  const digits = value.replace(/[\s ]/g, "").replace(",", ".");
  if (digits === "") return null;
  const n = Math.round(Number(digits));
  return Number.isFinite(n) ? n : null;
}

const orNull = (value: string) => (value.trim() === "" ? null : value.trim());

export function AddOfferDialog({
  open,
  shared,
  aiOn,
  onClose,
  onSave,
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
  onSave: (offer: OfferInput) => Promise<boolean>;
  onOpenPreview: (url: string) => Promise<boolean>;
  onCapture: () => Promise<Captured | null>;
  onFill: (text: string, title: string | null) => Promise<Draft | null>;
}) {
  const { t } = useI18n();
  const { errors, formProps, field } = useFieldErrors(open);
  const [form, setForm] = useState(EMPTY);
  const [busy, setBusy] = useState(false);
  const [link, setLink] = useState("");
  const [pasted, setPasted] = useState("");
  const [pageTitle, setPageTitle] = useState<string | null>(null);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [working, setWorking] = useState<"capture" | "fill" | null>(null);
  const [filledBy, setFilledBy] = useState<"ai" | "code" | null>(null);

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
      description: draft.description ?? f.description,
    }));
    setFilledBy(draft.byAi ? "ai" : "code");
  }

  const set = <K extends keyof typeof EMPTY>(key: K, value: (typeof EMPTY)[K]) =>
    setForm((f) => ({ ...f, [key]: value }));

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    const ok = await onSave({
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
    });
    setBusy(false);
    if (ok) {
      setForm(EMPTY);
      setLink("");
      setPasted("");
      setPageTitle(null);
      setFilledBy(null);
      onClose();
    }
  }

  return (
    <Modal open={open} onClose={onClose} labelledBy="add-title" className="modal-wide">
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
                <input type="url" maxLength={2000} placeholder="https://" value={link} onChange={(e) => setLink(e.target.value)} />
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
            <textarea rows={3} maxLength={100000} value={pasted} onChange={(e) => setPasted(e.target.value)} />
          </label>
          <div className="quick-row">
            <span className="hint" role="status">
              {filledBy === "ai" ? t("quick.byAi") : filledBy === "code" ? t("quick.byCode") : ""}
            </span>
            <button type="button" className="btn btn-ghost btn-small" disabled={!pasted.trim() || working !== null} aria-busy={working === "fill"} onClick={() => fill()}>
              {working === "fill" ? t("ai.working") : aiOn ? t("quick.fillAi") : t("quick.fill")}
            </button>
          </div>
        </fieldset>

        <div className="form-grid">
          <label className="field span-2">
            <span>
              {t("add.fieldTitle")} <em className="req">({t("add.required")})</em>
            </span>
            <input required maxLength={300} value={form.title} onChange={(e) => set("title", e.target.value)} data-autofocus={!isPhone() || undefined} {...field("offer-title")} />
            <FieldError name="offer-title" />
          </label>
          <label className="field span-2">
            <span>
              {t("add.fieldCompany")} <em className="req">({t("add.required")})</em>
            </span>
            <input required maxLength={300} value={form.company} onChange={(e) => set("company", e.target.value)} {...field("offer-company")} />
            <FieldError name="offer-company" />
          </label>
          <label className="field span-2">
            <span>{t("add.fieldUrl")}</span>
            <input
              type="url"
              pattern="https?://.+"
              maxLength={2000}
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
            <input maxLength={300} value={form.city} onChange={(e) => set("city", e.target.value)} />
          </label>
          <label className="field">
            <span>{t("add.fieldRemote")}</span>
            <select value={form.remote} onChange={(e) => set("remote", e.target.value as Remote)}>
              {(["unknown", "yes", "hybrid", "no"] as Remote[]).map((r) => (
                <option key={r} value={r}>
                  {t(`remote.${r}`)}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>{t("add.fieldSalaryMin")}</span>
            <input inputMode="numeric" value={form.salaryMin} onChange={(e) => set("salaryMin", e.target.value)} />
          </label>
          <label className="field">
            <span>{t("add.fieldSalaryMax")}</span>
            <input inputMode="numeric" value={form.salaryMax} onChange={(e) => set("salaryMax", e.target.value)} />
          </label>
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
            <input maxLength={300} list="contracts" value={form.contract} onChange={(e) => set("contract", e.target.value)} />
            <datalist id="contracts">
              <option value="UoP" />
              <option value="B2B" />
              <option value={t("contract.mandate")} />
            </datalist>
          </label>
          <label className="field">
            <span>{t("add.fieldLevel")}</span>
            <input maxLength={300} list="levels" value={form.level} onChange={(e) => set("level", e.target.value)} />
            <datalist id="levels">
              <option value="junior" />
              <option value="mid" />
              <option value="senior" />
            </datalist>
          </label>
          <label className="field span-2">
            <span>{t("add.fieldDescription")}</span>
            <textarea
              rows={6}
              maxLength={100000}
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
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            {t("add.cancel")}
          </button>
          <button type="submit" className="btn btn-sign" disabled={busy}>
            {t("add.save")}
          </button>
        </footer>
      </form>
      </FieldErrorsProvider>
    </Modal>
  );
}
