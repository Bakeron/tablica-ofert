import { useRef, useState } from "react";
import type { Api } from "../api";
import { describeError, type Profile, type SecretHints, type SecretName, type Settings, type SourceStatus } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { rememberUnfolded, settingsMemory } from "../lib/settingsMemory";
import { IconArrowOut, IconHome, IconWarning } from "./Icons";
import { Reveal } from "./Reveal";
import { SecretField } from "./SecretField";
import { TextSetting } from "./TextSetting";
import { JOOBLE_KEY_CALLS, TEXT_MAX } from "../lib/limits";
import { SourceBox, SourceState } from "./SourcesSettings";

/** Name of the "How to set up alerts" part in the memory of the Settings window. */
const HOWTO = "mail-howto";
/** Name of the Jooble API part in the same memory. */
const JOOBLE_API = "mail-jooble-api";
/** Key form on the Polish Jooble site - the key decides the country of the offers. */
const JOOBLE_SIGNUP = "https://pl.jooble.org/api/about";
const JOOBLE = "Jooble";

/** Portals that send alerts by e-mail - where to set them up (always by a person, once).
 *  `search` builds a ready search for one keyword in the whole country (checked 3.11.2026);
 *  the city, work mode and salary are checked later by the app's own filter. */
const PORTALS: { name: string; url: string; search: (q: string) => string }[] = [
  { name: "Just Join IT", url: "https://justjoin.it", search: (q) => `https://justjoin.it/job-offers/all-locations?keyword=${q}` },
  { name: "No Fluff Jobs", url: "https://nofluffjobs.com/pl", search: (q) => `https://nofluffjobs.com/pl/?criteria=keyword%3D${q}` },
  { name: "Pracuj.pl", url: "https://www.pracuj.pl", search: (q) => `https://www.pracuj.pl/praca/${q};kw` },
  { name: "theprotocol.it", url: "https://theprotocol.it", search: (q) => `https://theprotocol.it/praca?kw=${q}` },
  { name: "Rocket Jobs", url: "https://rocketjobs.pl", search: (q) => `https://rocketjobs.pl/oferty-pracy/wszystkie-lokalizacje?keyword=${q}` },
  { name: "Bulldogjob", url: "https://bulldogjob.pl", search: (q) => `https://bulldogjob.pl/companies/jobs/s/skills,${q}` },
  // geoId = Poland; the name alone may be resolved by the visitor's country.
  { name: "LinkedIn", url: "https://www.linkedin.com/jobs", search: (q) => `https://www.linkedin.com/jobs/search/?keywords=${q}&location=Poland&geoId=105072130` },
  { name: "Indeed", url: "https://pl.indeed.com", search: (q) => `https://pl.indeed.com/jobs?q=${q}` },
  { name: JOOBLE, url: "https://pl.jooble.org", search: (q) => `https://pl.jooble.org/SearchResult?ukw=${q}` },
];

/** At most this many search buttons per portal - one alert per keyword is the usual setup. */
const MAX_SEARCHES = 8;

/** The words for alerts (comma separated) or, when empty, the keywords of all profiles -
 *  without repeats (case does not matter). The same rule as `settings::search_words` in Rust. */
function searchWords(setting: string, profiles: Profile[]): string[] {
  const own = setting.split(",");
  const list = own.some((w) => w.trim()) ? own : profiles.flatMap((p) => p.criteria.keywords);
  const seen = new Set<string>();
  const words: string[] = [];
  for (const raw of list) {
    const k = raw.trim();
    if (!k || seen.has(k.toLowerCase())) continue;
    seen.add(k.toLowerCase());
    words.push(k);
  }
  return words.slice(0, MAX_SEARCHES);
}

const GMAIL_HOST = "imap.gmail.com";
const APP_PASSWORDS_URL = "https://myaccount.google.com/apppasswords";

const WORK_MODE_KEYS: Record<string, TextKey> = { yes: "remote.yes", hybrid: "remote.hybrid", no: "remote.no" };

/** Settings → "Offer sources" → e-mail alerts: mailbox, warning, how to set up alerts. */
export function MailSettings({
  api,
  settings,
  status,
  jooble,
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
  jooble: SourceStatus | undefined;
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
  const [joobleOpen, setJoobleOpen] = useState(() => settingsMemory.unfolded.has(JOOBLE_API));
  // The result stays under the button - the app's error bar is behind this window.
  const howtoRef = useRef<HTMLDetailsElement>(null);
  const isGmail = settings.mail_host.trim().toLowerCase() === GMAIL_HOST;
  const words = searchWords(settings.search_words, profiles);
  const [result, setResult] = useState<{ ok: boolean; text: string; detail?: string; appPassword?: boolean } | null>(null);

  async function test() {
    setTesting(true);
    setResult(null);
    try {
      const count = await api.mailTest();
      setResult({ ok: true, text: t("mail.testOk", { count }) });
      // The mailbox works - setting up alerts on the portals is what comes next.
      if (howtoRef.current) howtoRef.current.open = true;
    } catch (e) {
      const { key, detail } = describeError(e);
      // Gmail refuses the normal Google password over IMAP - only an app password works.
      const gmail = key === "error.auth" && isGmail;
      setResult({ ok: false, text: t(gmail ? "mail.gmailAuth" : (key as TextKey)), detail, appPassword: gmail });
    } finally {
      setTesting(false);
    }
  }

  return (
    <SourceBox
      id="src-mail"
      name={t("mail.title")}
      short={t("mail.short")}
      on={settings.source_mail === "on"}
      onToggle={(on) => onChange("source_mail", on ? "on" : "off")}
      state={status}
      lang={lang}
      // The Jooble API sits deep in this box - its problem must show with everything folded.
      alert={jooble?.enabled ? <SourceState s={jooble} lang={lang} onlyProblems name={JOOBLE} /> : undefined}
    >
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
          maxLength={TEXT_MAX.mail_user}
          hint={t("mail.userHint")}
          value={settings.mail_user}
          type="email"
          onSave={(v) => onChange("mail_user", v)}
        />
        <SecretField name="mail-password" label={t(isGmail ? "mail.passwordGmail" : "mail.password")} hint={hints["mail-password"]} onSave={onSaveSecret} onDelete={onDeleteSecret} onReveal={api.revealSecret} />
        <TextSetting label={t("mail.host")} maxLength={TEXT_MAX.mail_host} value={settings.mail_host} onSave={(v) => onChange("mail_host", v)} />
        <TextSetting label={t("mail.port")} maxLength={TEXT_MAX.mail_port} value={settings.mail_port} inputMode="numeric" onSave={(v) => onChange("mail_port", v)} />
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
              <p className="source-state">
                {result.text}
                <br />
                {t("mail.nextStep")}
              </p>
            ) : (
              <div className="source-warn">
                <IconWarning />
                <div>
                  {result.text}
                  {result.appPassword && (
                    <div>
                      <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(APP_PASSWORDS_URL)}>
                        {t("mail.gmailAuthOpen")} <IconArrowOut />
                      </button>
                    </div>
                  )}
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
      <TextSetting
        label={t("mail.searchWords")}
        maxLength={TEXT_MAX.search_words}
        hint={t("mail.searchWordsHint")}
        value={settings.search_words}
        placeholder={t("mail.searchWordsPlaceholder")}
        onSave={(v) => onChange("search_words", v)}
      />

      <details
        className="howto"
        ref={howtoRef}
        open={settingsMemory.unfolded.has(HOWTO)}
        onToggle={(e) => rememberUnfolded(HOWTO, e.currentTarget.open)}
      >
        <summary>{t("mail.howto")}</summary>
        <p>{t("mail.howtoText")}</p>
        <AlertData email={settings.mail_user} profiles={profiles} />
        {words.length > 0 && <p className="hint">{t("mail.searchHint")}</p>}
        <ul className={words.length > 0 ? "portal-list portal-list-search" : "portal-list"}>
          {PORTALS.map((p) => (
            <li key={p.name} className={p.name === JOOBLE ? "portal-wide" : undefined}>
              <span>{p.name}</span>
              <div className="portal-links">
                {words.length > 0 ? (
                  <button
                    type="button"
                    className="btn btn-ghost btn-small btn-icon-only"
                    aria-label={t("mail.openHome", { portal: p.name })}
                    title={t("mail.openHome", { portal: p.name })}
                    onClick={() => onOpenLink(p.url)}
                  >
                    <IconHome />
                  </button>
                ) : (
                  <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(p.url)}>
                    {t("mail.openPortal")} <IconArrowOut />
                  </button>
                )}
                {words.map((w) => (
                  <button
                    key={w}
                    type="button"
                    className="btn btn-ghost btn-small"
                    aria-label={t("mail.searchLabel", { portal: p.name, word: w })}
                    onClick={() => onOpenLink(p.search(encodeURIComponent(w)))}
                  >
                    {w} <IconArrowOut />
                  </button>
                ))}
                {p.name === JOOBLE && (
                  <button
                    type="button"
                    className="btn btn-ghost btn-small"
                    aria-expanded={joobleOpen}
                    aria-controls="jooble-api"
                    onClick={() => {
                      rememberUnfolded(JOOBLE_API, !joobleOpen);
                      setJoobleOpen(!joobleOpen);
                    }}
                  >
                    {t("src.jooble.api")}
                  </button>
                )}
              </div>
              {p.name === JOOBLE && (
                <Reveal show={joobleOpen} className="portal-more">
                  <div id="jooble-api" className="source-more" role="region" aria-label={t("src.more", { name: JOOBLE })}>
                    <p>{t("src.jooble.text", { calls: JOOBLE_KEY_CALLS })}</p>
                    <label className="choice-item">
                      <input
                        type="checkbox"
                        checked={settings.source_jooble === "on"}
                        onChange={(e) => onChange("source_jooble", e.target.checked ? "on" : "off")}
                      />
                      <span>{t("src.jooble.use")}</span>
                    </label>
                    <div className="source-row">
                      <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(JOOBLE_SIGNUP)}>
                        {t("src.jooble.signup")} <IconArrowOut />
                      </button>
                    </div>
                    <SecretField name="jooble-key" label={t("src.jooble.key")} hint={hints["jooble-key"]} onSave={onSaveSecret} onDelete={onDeleteSecret} onReveal={api.revealSecret} />
                    <p className="hint">{t("secret.hint")}</p>
                    {jooble && <SourceState s={jooble} lang={lang} noProblems />}
                  </div>
                </Reveal>
              )}
            </li>
          ))}
        </ul>
        <p className="hint">{t("mail.forward")}</p>
      </details>
    </SourceBox>
  );

  /** Ready-to-copy data for the alert form, from the profiles. */
  function AlertData({ email, profiles }: { email: string; profiles: Profile[] }) {
    const rows: { label: string; value: string }[] = [];
    if (email) rows.push({ label: t("mail.data.email"), value: email });
    // Own words for alerts replace the profile keywords here too.
    const ownWords = settings.search_words.trim() !== "";
    if (ownWords) rows.push({ label: t("mail.searchWords"), value: words.join(", ") });
    for (const p of profiles) {
      const c = p.criteria;
      if (!ownWords && c.keywords.length) rows.push({ label: `${p.name} - ${t("mail.data.keywords")}`, value: c.keywords.join(", ") });
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
