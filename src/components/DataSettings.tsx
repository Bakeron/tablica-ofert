import { useState, type FormEvent } from "react";
import type { Api } from "../api";
import { useI18n } from "../i18n";
import { IconWarning } from "./Icons";

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
  const { t } = useI18n();
  const [savePass, setSavePass] = useState("");
  const [savePass2, setSavePass2] = useState("");
  const [loadPass, setLoadPass] = useState("");
  const [busy, setBusy] = useState(false);
  const mismatch = savePass2 !== "" && savePass !== savePass2;

  async function save(e: FormEvent) {
    e.preventDefault();
    if (mismatch || savePass.length < 8) return;
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
      const path = await api.exportCsv();
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
          <label className="field">
            <span>{t("data.password")}</span>
            <input type="password" autoComplete="new-password" minLength={8} maxLength={200} value={savePass} onChange={(e) => setSavePass(e.target.value)} />
          </label>
          <label className="field">
            <span>{t("data.password2")}</span>
            <input type="password" autoComplete="new-password" maxLength={200} value={savePass2} aria-invalid={mismatch} onChange={(e) => setSavePass2(e.target.value)} />
            {mismatch && <span className="source-warn">{t("data.mismatch")}</span>}
          </label>
        </div>
        <p className="hint">{t("data.passwordHint")}</p>
        <div>
          <button type="submit" className="btn btn-sign btn-small" disabled={busy || mismatch || savePass.length < 8 || savePass2 === ""}>
            {t("data.save")}
          </button>
        </div>
      </form>

      <form className="source" onSubmit={load} aria-labelledby="load-title">
        <h3 id="load-title" className="source-title">{t("data.loadTitle")}</h3>
        <p>{t("data.loadText")}</p>
        <label className="field field-inline">
          <span>{t("data.password")}</span>
          <input type="password" autoComplete="off" maxLength={200} value={loadPass} onChange={(e) => setLoadPass(e.target.value)} />
        </label>
        <div>
          <button type="submit" className="btn btn-ghost btn-small" disabled={busy || !loadPass}>
            {t("data.load")}
          </button>
        </div>
      </form>

      <section className="source" aria-labelledby="csv-title">
        <h3 id="csv-title" className="source-title">{t("data.csvTitle")}</h3>
        <p>{t("data.csvText")}</p>
        <div>
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
