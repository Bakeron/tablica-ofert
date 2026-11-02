import { useCallback, useEffect, useState } from "react";
import type { Api } from "../api";
import { ADZUNA_COUNTRIES, type Profile, type SecretHints, type SecretName, type Settings, type SourceStatus } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { fullDate } from "../lib/format";
import { IconArrowOut, IconWarning } from "./Icons";
import { MailSettings } from "./MailSettings";
import { SecretField } from "./SecretField";

const ADZUNA_SIGNUP = "https://developer.adzuna.com/signup";

/** Settings → "Offer sources": automatic fetching, Adzuna (with its key), Remotive, e-mail alerts. */
export function SourcesSettings({
  api,
  settings,
  profiles,
  onChange,
  onOpenLink,
  onError,
  onToast,
}: {
  api: Api;
  settings: Settings;
  profiles: Profile[];
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onOpenLink: (url: string) => void;
  onError: (e: unknown) => void;
  onToast: (text: string) => void;
}) {
  const { t, lang } = useI18n();
  const [status, setStatus] = useState<SourceStatus[]>([]);
  const [hints, setHints] = useState<SecretHints>({});

  const load = useCallback(async () => {
    try {
      const [s, h] = await Promise.all([api.sourceStatus(), api.secretHints()]);
      setStatus(s);
      setHints(h);
    } catch (e) {
      onError(e);
    }
  }, [api, onError]);

  useEffect(() => {
    load();
  }, [load, settings.source_adzuna, settings.source_remotive, settings.source_mail, settings.mail_user, settings.mail_host]);

  async function saveSecret(name: SecretName, value: string) {
    try {
      await api.setSecret(name, value);
      onToast(t("secret.saved"));
      await load();
      return true;
    } catch (e) {
      onError(e);
      return false;
    }
  }

  async function deleteSecret(name: SecretName) {
    try {
      await api.deleteSecret(name);
      onToast(t("secret.deleted"));
      await load();
      return true;
    } catch (e) {
      onError(e);
      return false;
    }
  }

  const of = (source: SourceStatus["source"]) => status.find((s) => s.source === source);
  const adzuna = of("adzuna");
  const remotive = of("remotive");

  return (
    <div className="settings-part">
      <fieldset className="crit-group">
        <label className="choice-item">
          <input
            type="checkbox"
            checked={settings.fetch_auto === "on"}
            onChange={(e) => onChange("fetch_auto", e.target.checked ? "on" : "off")}
          />
          <span>{t("src.auto")}</span>
        </label>
        {settings.fetch_auto === "on" && (
          <label className="field field-inline">
            <span>{t("src.every")}</span>
            <select value={settings.fetch_every} onChange={(e) => onChange("fetch_every", e.target.value as Settings["fetch_every"])}>
              {(["3", "6", "12"] as const).map((h) => (
                <option key={h} value={h}>
                  {t("src.every.value", { count: h })}
                </option>
              ))}
            </select>
          </label>
        )}
        <p className="hint">{t("src.keywordsNote")}</p>
      </fieldset>

      <section className="source" aria-labelledby="src-adzuna">
        <SourceHead id="src-adzuna" name="Adzuna" on={settings.source_adzuna === "on"} onToggle={(on) => onChange("source_adzuna", on ? "on" : "off")} />
        <p>{t("src.adzuna.text")}</p>
        <div className="source-row">
          <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(ADZUNA_SIGNUP)}>
            {t("src.adzuna.signup")} <IconArrowOut />
          </button>
          <label className="field field-inline">
            <span>{t("src.adzuna.country")}</span>
            <select value={settings.adzuna_country} onChange={(e) => onChange("adzuna_country", e.target.value as Settings["adzuna_country"])}>
              {ADZUNA_COUNTRIES.map((c) => (
                <option key={c} value={c}>
                  {t(`country.${c}` as TextKey)}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="secret-pair">
          <SecretField name="adzuna-app-id" label={t("src.adzuna.appId")} hint={hints["adzuna-app-id"]} onSave={saveSecret} onDelete={deleteSecret} />
          <SecretField name="adzuna-app-key" label={t("src.adzuna.appKey")} hint={hints["adzuna-app-key"]} onSave={saveSecret} onDelete={deleteSecret} />
        </div>
        <p className="hint">{t("secret.hint")}</p>
        {adzuna && <SourceState s={adzuna} lang={lang} />}
      </section>

      <section className="source" aria-labelledby="src-remotive">
        <SourceHead id="src-remotive" name="Remotive" on={settings.source_remotive === "on"} onToggle={(on) => onChange("source_remotive", on ? "on" : "off")} />
        <p>{t("src.remotive.text")}</p>
        {remotive && <SourceState s={remotive} lang={lang} />}
      </section>

      <MailSettings
        api={api}
        settings={settings}
        status={of("mail")}
        hints={hints}
        profiles={profiles}
        onChange={onChange}
        onSaveSecret={saveSecret}
        onDeleteSecret={deleteSecret}
        onOpenLink={onOpenLink}
        onError={onError}
        onToast={onToast}
      />
    </div>
  );
}

export function SourceHead({ id, name, on, onToggle }: { id: string; name: string; on: boolean; onToggle: (on: boolean) => void }) {
  const { t } = useI18n();
  return (
    <div className="source-head">
      <h3 id={id}>{name}</h3>
      <label className="chip">
        <input type="checkbox" checked={on} onChange={(e) => onToggle(e.target.checked)} aria-label={`${name}: ${t("src.on")}`} />
        <span>{on ? t("src.on") : t("src.off")}</span>
      </label>
    </div>
  );
}

/** Last run, calls today and the last error of a source - in words, not only colour. */
export function SourceState({ s, lang }: { s: SourceStatus; lang: "pl" | "en" }) {
  const { t } = useI18n();
  const failed = s.lastError && s.lastTry && s.lastTry !== s.lastOk;
  return (
    <div className="source-state">
      {!s.ready && (
        <p className="source-warn">
          <IconWarning /> {t(s.source === "mail" ? "src.status.noMail" : "src.status.noKey")}
        </p>
      )}
      <p>
        {s.lastOk
          ? t("src.status.last", { when: fullDate(s.lastOk, lang), added: s.added, merged: s.merged })
          : t("src.status.never")}{" "}
        {s.callsADay > 0 && t("src.status.calls", { count: s.callsToday, max: s.callsADay })}
      </p>
      {failed && (
        <p className="source-warn">
          <IconWarning /> {t("src.status.error", { when: fullDate(s.lastTry!, lang), detail: s.lastError! })}
        </p>
      )}
    </div>
  );
}
