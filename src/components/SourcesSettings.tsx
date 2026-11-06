import { useCallback, useEffect, useState, type ReactNode } from "react";
import type { Api } from "../api";
import { ADZUNA_COUNTRIES, type Profile, type SecretHints, type SecretName, type Settings, type SourceStatus, type TaskStatus } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { fullDate } from "../lib/format";
import { rememberUnfolded, settingsMemory } from "../lib/settingsMemory";
import { FOLLOW_UP_DAYS, JOOBLE_KEY_CALLS } from "../lib/limits";
import { IconArrowOut, IconArrowRight, IconWarning } from "./Icons";
import { Reveal } from "./Reveal";
import { MailSettings } from "./MailSettings";
import { SecretField } from "./SecretField";

const ADZUNA_SIGNUP = "https://developer.adzuna.com/signup";

/** Settings → "Offer sources": automatic fetching, Adzuna (with its keys), Remotive, e-mail alerts (with Jooble among the portals). */
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
  const [task, setTask] = useState<TaskStatus | null>(null);
  const [taskBusy, setTaskBusy] = useState(false);

  useEffect(() => {
    api.backgroundStatus().then(setTask, onError);
  }, [api, onError]);

  async function toggleBackground(on: boolean) {
    setTaskBusy(true);
    try {
      setTask(await api.setBackground(on));
      onChange("background_task", on ? "on" : "off");
    } catch (e) {
      onError(e);
    } finally {
      setTaskBusy(false);
    }
  }

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
  }, [load, settings.source_adzuna, settings.source_remotive, settings.source_himalayas, settings.source_weworkremotely, settings.source_remoteok, settings.source_jooble, settings.source_mail, settings.mail_user, settings.mail_host]);

  async function saveSecret(name: SecretName, value: string) {
    // An error is shown by the field itself, under the input (the app's error bar is behind this window).
    await api.setSecret(name, value);
    onToast(t("secret.saved"));
    await load();
    return true;
  }

  async function deleteSecret(name: SecretName) {
    // An error is shown by the field itself, under the input (the app's error bar is behind this window).
    await api.deleteSecret(name);
    onToast(t("secret.deleted"));
    await load();
    return true;
  }

  const of = (source: SourceStatus["source"]) => status.find((s) => s.source === source);
  const adzuna = of("adzuna");
  const remotive = of("remotive");
  const himalayas = of("himalayas");
  const wwr = of("weworkremotely");
  const remoteok = of("remoteok");

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
        <label className="choice-item">
          <input type="checkbox" checked={settings.background_task === "on"} disabled={taskBusy} onChange={(e) => toggleBackground(e.target.checked)} />
          <span>{t("bg.label")}</span>
        </label>
        <p className="hint">
          {t("bg.hint")}{" "}
          {task && <strong>{task.exists ? t("bg.exists") : t("bg.missing")}</strong>}
        </p>
        <label className="choice-item">
          <input type="checkbox" checked={settings.notify_new === "on"} onChange={(e) => onChange("notify_new", e.target.checked ? "on" : "off")} />
          <span>{t("bg.notify")}</span>
        </label>
        <label className="choice-item">
          <input type="checkbox" checked={settings.notify_reminders === "on"} onChange={(e) => onChange("notify_reminders", e.target.checked ? "on" : "off")} />
          <span>{t("bg.reminders", { days: FOLLOW_UP_DAYS })}</span>
        </label>
      </fieldset>

      <SourceBox id="src-adzuna" name="Adzuna" short={t("src.adzuna.short")} on={settings.source_adzuna === "on"} onToggle={(on) => onChange("source_adzuna", on ? "on" : "off")} state={adzuna} lang={lang}>
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
          <SecretField name="adzuna-app-id" label={t("src.adzuna.appId")} hint={hints["adzuna-app-id"]} onSave={saveSecret} onDelete={deleteSecret} onReveal={api.revealSecret} />
          <SecretField name="adzuna-app-key" label={t("src.adzuna.appKey")} hint={hints["adzuna-app-key"]} onSave={saveSecret} onDelete={deleteSecret} onReveal={api.revealSecret} />
        </div>
        <p className="hint">{t("secret.hint")}</p>
      </SourceBox>

      <SourceBox id="src-remotive" name="Remotive" short={t("src.remotive.short")} on={settings.source_remotive === "on"} onToggle={(on) => onChange("source_remotive", on ? "on" : "off")} state={remotive} lang={lang}>
        <p>{t("src.remotive.text")}</p>
      </SourceBox>

      <SourceBox id="src-himalayas" name="Himalayas" short={t("src.himalayas.short")} on={settings.source_himalayas === "on"} onToggle={(on) => onChange("source_himalayas", on ? "on" : "off")} state={himalayas} lang={lang}>
        <p>{t("src.himalayas.text")}</p>
      </SourceBox>

      <SourceBox
        id="src-wwr"
        name="We Work Remotely"
        short={t("src.wwr.short")}
        on={settings.source_weworkremotely === "on"}
        onToggle={(on) => onChange("source_weworkremotely", on ? "on" : "off")}
        state={wwr}
        lang={lang}
      >
        <p>{t("src.wwr.text")}</p>
      </SourceBox>

      <SourceBox id="src-remoteok" name="Remote OK" short={t("src.remoteok.short")} on={settings.source_remoteok === "on"} onToggle={(on) => onChange("source_remoteok", on ? "on" : "off")} state={remoteok} lang={lang}>
        <p>{t("src.remoteok.text")}</p>
      </SourceBox>

      <MailSettings
        api={api}
        settings={settings}
        status={of("mail")}
        jooble={of("jooble")}
        hints={hints}
        profiles={profiles}
        onChange={onChange}
        onSaveSecret={saveSecret}
        onDeleteSecret={deleteSecret}
        onChecked={load}
        onOpenLink={onOpenLink}
        onToast={onToast}
      />
    </div>
  );
}

/**
 * One source: the name with "Show settings" (a click unfolds the rest), the on / off switch and one short line are
 * always visible; the long description, keys and the last run show after unfolding. A
 * problem (missing key, last error) shows folded too - it is never hidden.
 */
export function SourceBox({
  id,
  name,
  short,
  on,
  onToggle,
  state,
  lang = "pl",
  alert,
  children,
}: {
  id: string;
  name: string;
  short: string;
  on: boolean;
  onToggle: (on: boolean) => void;
  state?: SourceStatus;
  lang?: "pl" | "en";
  /** A problem shown folded and unfolded (for boxes without `state`). */
  alert?: ReactNode;
  children: ReactNode;
}) {
  const { t } = useI18n();
  const [open, setOpen] = useState(() => settingsMemory.unfolded.has(id));
  const toggle = () => {
    rememberUnfolded(id, !open);
    setOpen(!open);
  };
  return (
    <section className="source" aria-labelledby={id} data-open={open || undefined}>
      <div className="source-head">
        <h3 id={id}>
          <button type="button" className="source-unfold" aria-expanded={open} aria-controls={`${id}-more`} onClick={toggle}>
            <IconArrowRight />
            {name}
            <span className="source-unfold-hint">{open ? t("src.hide") : t("src.show")}</span>
          </button>
        </h3>
        <label className="chip">
          <input type="checkbox" checked={on} onChange={(e) => onToggle(e.target.checked)} aria-label={`${name}: ${t("src.on")}`} />
          <span>{on ? t("src.on") : t("src.off")}</span>
        </label>
      </div>
      <p className="source-short">{short}</p>
      {alert}
      {!open && state && <SourceState s={state} lang={lang} onlyProblems />}
      <Reveal show={open}>
        <div id={`${id}-more`} className="source-more" role="region" aria-label={t("src.more", { name })}>
          {children}
          {state && <SourceState s={state} lang={lang} />}
        </div>
      </Reveal>
    </section>
  );
}

/** Last run, calls today and the last error of a source - in words, not only colour.
 *  `name` starts the warnings when they stand outside the box of the source; `noProblems` leaves them to that place. */
export function SourceState({
  s,
  lang,
  onlyProblems = false,
  noProblems = false,
  name,
}: {
  s: SourceStatus;
  lang: "pl" | "en";
  onlyProblems?: boolean;
  noProblems?: boolean;
  name?: string;
}) {
  const { t } = useI18n();
  // A successful run clears the error; the Himalayas check of closed jobs can set it after one.
  const failed = !noProblems && s.lastError && s.lastTry;
  const noKey = !noProblems && !s.ready && s.enabled;
  const prefix = name ? `${name}: ` : "";
  if (onlyProblems && !failed && !noKey) return null;
  return (
    <div className="source-state">
      {/* A missing key is a problem only for a source that is on - "not set" under the field says the rest. */}
      {noKey && (
        <p className="source-warn">
          <IconWarning /> {prefix}
          {t(s.source === "mail" ? "src.status.noMail" : "src.status.noKey")}
        </p>
      )}
      {!onlyProblems && (
        <p>
          {s.lastOk
            ? t("src.status.last", { when: fullDate(s.lastOk, lang), added: s.added, merged: s.merged })
            : t("src.status.never")}{" "}
          {s.callsADay > 0 && t("src.status.calls", { count: s.callsToday, max: s.callsADay })}{" "}
          {s.callsLeft !== null && t("src.status.callsLeft", { count: s.callsLeft, calls: JOOBLE_KEY_CALLS })}
        </p>
      )}
      {failed && (
        <p className="source-warn">
          <IconWarning /> {prefix}
          {t("src.status.error", { when: fullDate(s.lastTry!, lang), detail: s.lastError! })}
        </p>
      )}
    </div>
  );
}
