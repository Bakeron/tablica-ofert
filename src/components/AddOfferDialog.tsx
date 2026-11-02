import { useState, type FormEvent } from "react";
import type { OfferInput, Remote, SalaryPeriod } from "../api/types";
import { useI18n } from "../i18n";
import { IconClose } from "./Icons";
import { Modal } from "./Modal";

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
  onClose,
  onSave,
}: {
  open: boolean;
  onClose: () => void;
  onSave: (offer: OfferInput) => Promise<boolean>;
}) {
  const { t } = useI18n();
  const [form, setForm] = useState(EMPTY);
  const [busy, setBusy] = useState(false);

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
      onClose();
    }
  }

  return (
    <Modal open={open} onClose={onClose} labelledBy="add-title" className="modal-wide">
      <form onSubmit={submit} className="sheet">
        <header className="sheet-head">
          <h2 id="add-title">{t("add.title")}</h2>
          <button type="button" className="icon-btn" onClick={onClose} aria-label={t("add.cancel")}>
            <IconClose />
          </button>
        </header>
        <p className="hint">{t("add.intro")}</p>

        <div className="form-grid">
          <label className="field span-2">
            <span>
              {t("add.fieldTitle")} <em className="req">({t("add.required")})</em>
            </span>
            <input required maxLength={300} value={form.title} onChange={(e) => set("title", e.target.value)} data-autofocus />
          </label>
          <label className="field span-2">
            <span>
              {t("add.fieldCompany")} <em className="req">({t("add.required")})</em>
            </span>
            <input required maxLength={300} value={form.company} onChange={(e) => set("company", e.target.value)} />
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
            />
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
              pattern="[A-Za-z]{3}"
              value={form.salaryCurrency}
              onChange={(e) => set("salaryCurrency", e.target.value.toUpperCase())}
            />
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
              <option value="Umowa zlecenie" />
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
    </Modal>
  );
}
