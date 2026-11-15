import { useId, useState, type FormEvent } from "react";
import type { Api } from "../api";
import { useI18n } from "../i18n";
import { CSV_LISTS, csvCells, csvName, csvSeparator, type CsvList } from "../lib/csv";
import { placeName } from "../lib/profile";
import { PASSWORD_MAX, PASSWORD_MIN } from "../lib/limits";
import { IconWarning } from "./Icons";
import { ShowToggle } from "./SecretField";

/** Settings → "Data": a copy with a password, loading a copy, CSV export, deleting everything. */
export function DataSettings({
  api,
  onError,
  onToast,
  onConfirm,
  onLoaded,
}: {
  api: Api;
  onError: (e: unknown) => void;
  onToast: (text: string) => void;
  /** Asks before anything is replaced or deleted. */
  onConfirm: (message: string, action: () => void, label?: string) => void;
  /** A copy was loaded - every list must be read again. */
  onLoaded: () => void;
}) {
  const { t, lang } = useI18n();
  const [savePass, setSavePass] = useState("");
  const [savePass2, setSavePass2] = useState("");
  const [loadPass, setLoadPass] = useState("");
  const id = useId();
  const [shown, setShown] = useState({ save: false, save2: false, load: false });
  const [busy, setBusy] = useState(false);
  // "Oferty" first - the list that matters most.
  const [csvList, setCsvList] = useState<CsvList>("board");
  const mismatch = savePass2 !== "" && savePass !== savePass2;

  async function save(e: FormEvent) {
    e.preventDefault();
    if (mismatch || [...savePass].length < PASSWORD_MIN) return;
    setBusy(true);
    try {
      const path = await api.saveCopy(savePass);
      if (path) {
        onToast(t("data.saved", { path }));
        setSavePass("");
        setSavePass2("");
      }
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  }

  function load(e: FormEvent) {
    e.preventDefault();
    if (!loadPass) return;
    onConfirm(t("data.loadConfirm"), async () => {
      setBusy(true);
      try {
        const count = await api.loadCopy(loadPass);
        if (count !== null) {
          onToast(t("data.loaded", { count }));
          setLoadPass("");
          onLoaded();
        }
      } catch (err) {
        onError(err);
      } finally {
        setBusy(false);
      }
    }, t("data.load"));
  }

  async function csv() {
    setBusy(true);
    try {
      const rows = await api.exportRows(csvList === "all" ? null : csvList);
      const path = await api.saveCsv(csvName(csvList, lang), csvCells(rows, lang), csvSeparator(lang));
      if (path) onToast(t("data.csvSaved", { path }));
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="settings-part">
      <form className="source" onSubmit={save} aria-labelledby="copy-title">
        <h3 id="copy-title" className="source-title">{t("data.saveTitle")}</h3>
        <p>{t("data.saveText")}</p>
        <div className="mail-grid">
          <div className="field">
            <div className="secret-head">
              <label htmlFor={`${id}-save`}>{t("data.password")}</label>
              <ShowToggle controls={`${id}-save`} shown={shown.save} onToggle={(v) => setShown({ ...shown, save: v })} />
            </div>
            <input id={`${id}-save`} type={shown.save ? "text" : "password"} autoComplete="new-password" minLength={PASSWORD_MIN} maxLength={PASSWORD_MAX} value={savePass} onChange={(e) => setSavePass(e.target.value)} />
          </div>
          <div className="field">
            <div className="secret-head">
              <label htmlFor={`${id}-save2`}>{t("data.password2")}</label>
              <ShowToggle controls={`${id}-save2`} shown={shown.save2} onToggle={(v) => setShown({ ...shown, save2: v })} />
            </div>
            <input id={`${id}-save2`} type={shown.save2 ? "text" : "password"} autoComplete="new-password" maxLength={PASSWORD_MAX} value={savePass2} aria-invalid={mismatch} onChange={(e) => setSavePass2(e.target.value)} />
            {mismatch && <span className="source-warn">{t("data.mismatch")}</span>}
          </div>
        </div>
        <p className="hint">{t("data.passwordHint", { min: PASSWORD_MIN })}</p>
        <div>
          <button type="submit" className="btn btn-sign btn-small" disabled={busy || mismatch || [...savePass].length < PASSWORD_MIN || savePass2 === ""}>
            {t("data.save")}
          </button>
        </div>
      </form>

      <form className="source" onSubmit={load} aria-labelledby="load-title">
        <h3 id="load-title" className="source-title">{t("data.loadTitle")}</h3>
        <p>{t("data.loadText")}</p>
        <div className="field field-inline">
          <div className="secret-head">
            <label htmlFor={`${id}-load`}>{t("data.password")}</label>
            <ShowToggle controls={`${id}-load`} shown={shown.load} onToggle={(v) => setShown({ ...shown, load: v })} />
          </div>
          <input id={`${id}-load`} type={shown.load ? "text" : "password"} autoComplete="off" maxLength={PASSWORD_MAX} value={loadPass} onChange={(e) => setLoadPass(e.target.value)} />
        </div>
        <div>
          <button type="submit" className="btn btn-ghost btn-small" disabled={busy || !loadPass}>
            {t("data.load")}
          </button>
        </div>
      </form>

      <section className="source" aria-labelledby="csv-title">
        <h3 id="csv-title" className="source-title">{t("data.csvTitle")}</h3>
        <p>{t("data.csvText")}</p>
        <div className="csv-row">
          <label className="field field-inline">
            <span>{t("data.csvWhat")}</span>
            <select value={csvList} onChange={(e) => setCsvList(e.target.value as CsvList)}>
              {CSV_LISTS.map((l) => (
                <option key={l} value={l}>
                  {l === "all" ? t("data.csvAll") : placeName(l, t)}
                </option>
              ))}
            </select>
          </label>
          <button type="button" className="btn btn-ghost btn-small" onClick={csv} disabled={busy}>
            {t("data.csv")}
          </button>
        </div>
      </section>

      <section className="source source-danger" aria-labelledby="wipe-title">
        <h3 id="wipe-title" className="source-title">{t("data.wipeTitle")}</h3>
        <p className="source-warn">
          <IconWarning /> {t("data.wipeText")}
        </p>
        <div>
          <button
            type="button"
            className="btn btn-danger btn-small"
            disabled={busy}
            onClick={() => onConfirm(t("data.wipeConfirm"), () => api.wipeAll().catch(onError), t("data.wipe"))}
          >
            {t("data.wipe")}
          </button>
        </div>
      </section>
    </div>
  );
}
