import { useState } from "react";
import type { Api } from "../api";
import { describeError, type Profile, type SecretHints, type SecretName, type Settings, type SourceStatus } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { IconArrowOut, IconWarning } from "./Icons";
import { Reveal } from "./Reveal";
import { SecretField } from "./SecretField";
import { TextSetting } from "./TextSetting";
import { SourceHead, SourceState } from "./SourcesSettings";

/** Portals that send alerts by e-mail - where to set them up (always by a person, once). */
const PORTALS: { name: string; url: string }[] = [
  { name: "Just Join IT", url: "https://justjoin.it" },
  { name: "No Fluff Jobs", url: "https://nofluffjobs.com/pl" },
  { name: "Pracuj.pl", url: "https://www.pracuj.pl" },
  { name: "theprotocol.it", url: "https://theprotocol.it" },
  { name: "Rocket Jobs", url: "https://rocketjobs.pl" },
  { name: "Bulldogjob", url: "https://bulldogjob.pl" },
  { name: "LinkedIn", url: "https://www.linkedin.com/jobs" },
  { name: "Indeed", url: "https://pl.indeed.com" },
];

const WORK_MODE_KEYS: Record<string, TextKey> = { yes: "remote.yes", hybrid: "remote.hybrid", no: "remote.no" };

/** Settings → "Offer sources" → e-mail alerts: mailbox, warning, how to set up alerts. */
export function MailSettings({
  api,
  settings,
  status,
  hints,
  profiles,
  onChange,
  onSaveSecret,
  onDeleteSecret,
  onOpenLink,
  onToast,
}: {
  api: Api;
  settings: Settings;
  status: SourceStatus | undefined;
  hints: SecretHints;
  profiles: Profile[];
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onSaveSecret: (name: SecretName, value: string) => Promise<boolean>;
  onDeleteSecret: (name: SecretName) => Promise<boolean>;
  onOpenLink: (url: string) => void;
  onToast: (text: string) => void;
}) {
  const { t, lang } = useI18n();
  const [testing, setTesting] = useState(false);
  // The result stays under the button - the app's error bar is behind this window.
  const [result, setResult] = useState<{ ok: boolean; text: string; detail?: string } | null>(null);

  async function test() {
    setTesting(true);
    setResult(null);
    try {
      const count = await api.mailTest();
      setResult({ ok: true, text: t("mail.testOk", { count }) });
    } catch (e) {
      const { key, detail } = describeError(e);
      setResult({ ok: false, text: t(key as TextKey), detail });
    } finally {
      setTesting(false);
    }
  }

  return (
    <section className="source" aria-labelledby="src-mail">
      <SourceHead id="src-mail" name={t("mail.title")} on={settings.source_mail === "on"} onToggle={(on) => onChange("source_mail", on ? "on" : "off")} />
      <p>{t("mail.text")}</p>

      <div className="warn-box" role="note">
        <IconWarning />
        <p>
          <strong>{t("mail.warnTitle")}: </strong>
          {t("mail.warn")}
        </p>
      </div>

      <div className="mail-grid">
        <TextSetting
          label={t("mail.user")}
          hint={t("mail.userHint")}
          value={settings.mail_user}
          type="email"
          onSave={(v) => onChange("mail_user", v)}
        />
        <SecretField name="mail-password" label={t("mail.password")} hint={hints["mail-password"]} onSave={onSaveSecret} onDelete={onDeleteSecret} onReveal={api.revealSecret} />
        <TextSetting label={t("mail.host")} value={settings.mail_host} onSave={(v) => onChange("mail_host", v)} />
        <TextSetting label={t("mail.port")} value={settings.mail_port} inputMode="numeric" onSave={(v) => onChange("mail_port", v)} />
      </div>
      <p className="hint">{t("mail.gmailHint")}</p>
      <div>
        <button type="button" className="btn btn-ghost btn-small" onClick={test} disabled={testing || !status?.ready} aria-busy={testing}>
          {testing ? t("mail.testing") : t("mail.test")}
        </button>
      </div>
      <div aria-live="polite">
        <Reveal show={result !== null}>
          {result &&
            (result.ok ? (
              <p className="source-state">{result.text}</p>
            ) : (
              <div className="source-warn">
                <IconWarning />
                <div>
                  {result.text}
                  {result.detail && (
                    <details>
                      <summary>{t("error.details")}</summary>
                      <code>{result.detail}</code>
                    </details>
                  )}
                </div>
              </div>
            ))}
        </Reveal>
      </div>
      {status && <SourceState s={status} lang={lang} />}

      <details className="howto">
        <summary>{t("mail.howto")}</summary>
        <p>{t("mail.howtoText")}</p>
        <AlertData email={settings.mail_user} profiles={profiles} />
        <ul className="portal-list">
          {PORTALS.map((p) => (
            <li key={p.name}>
              <span>{p.name}</span>
              <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(p.url)}>
                {t("mail.openPortal")} <IconArrowOut />
              </button>
            </li>
          ))}
        </ul>
        <p className="hint">{t("mail.forward")}</p>
      </details>
    </section>
  );

  /** Ready-to-copy data for the alert form, from the profiles. */
  function AlertData({ email, profiles }: { email: string; profiles: Profile[] }) {
    const rows: { label: string; value: string }[] = [];
    if (email) rows.push({ label: t("mail.data.email"), value: email });
    for (const p of profiles) {
      const c = p.criteria;
      if (c.keywords.length) rows.push({ label: `${p.name} - ${t("mail.data.keywords")}`, value: c.keywords.join(", ") });
      if (c.cities.length) rows.push({ label: `${p.name} - ${t("mail.data.city")}`, value: c.cities.map((x) => x.name).join(", ") });
      if (c.workModes.length) rows.push({ label: `${p.name} - ${t("mail.data.mode")}`, value: c.workModes.map((m) => t(WORK_MODE_KEYS[m] ?? "remote.unknown")).join(", ") });
    }
    if (rows.length === 0) return <p className="hint">{t("mail.data.none")}</p>;
    return (
      <dl className="copy-list">
        {rows.map((r) => (
          <div key={r.label}>
            <dt>{r.label}</dt>
            <dd>
              <span className="mono">{r.value}</span>
              <CopyButton value={r.value} />
            </dd>
          </div>
        ))}
      </dl>
    );
  }

  function CopyButton({ value }: { value: string }) {
    return (
      <button
        type="button"
        className="btn btn-ghost btn-small"
        onClick={() =>
          navigator.clipboard.writeText(value).then(
            () => onToast(t("mail.copied")),
            () => onToast(t("mail.copyFailed")),
          )
        }
      >
        {t("mail.copy")}
      </button>
    );
  }
}
