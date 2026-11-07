import { Fragment, useId, useMemo, useState } from "react";
import { useI18n } from "../i18n";
import { unwrapNotices } from "../lib/notices";
import { isPhone } from "../lib/platform";
import { privacyBlocks, type Inline } from "../lib/privacy";
import { IconWarning } from "./Icons";
import { Reveal } from "./Reveal";
import privacyPolicy from "../../PRIVACY.md?raw";

/** Main libraries and fonts with their licences; every library with its full text is in
 *  `public/third-party-notices.txt` (made by `npm run notices`). */
const LICENSES: [string, string][] = [
  ["Tauri", "MIT / Apache-2.0"],
  ["React", "MIT"],
  ["rusqlite", "MIT"],
  ["SQLCipher", "BSD-3-Clause"],
  ["SQLite", "public domain"],
  ["OpenSSL", "Apache-2.0"],
  ["reqwest, tokio, serde, time", "MIT / Apache-2.0"],
  ["imap, native-tls, rustls, mail-parser", "MIT / Apache-2.0 / ISC"],
  ["keyring-core", "MIT / Apache-2.0"],
  ["Barlow Condensed, Atkinson Hyperlegible Next", "SIL Open Font License 1.1"],
  ["AndroidX, Material Components (Android)", "Apache-2.0"],
  ["CameraX, Jackson, Kotlin coroutines (Android)", "Apache-2.0"],
  ["ML Kit, Google Play services (Android, QR code scanner)", "Google APIs Terms of Service"],
];

function Parts({ parts, onOpenLink }: { parts: Inline[]; onOpenLink: (url: string) => void }) {
  return parts.map((p, i) => {
    if (p.kind === "strong") return <strong key={i}>{p.text}</strong>;
    if (p.kind === "code") return <code key={i}>{p.text}</code>;
    if (p.kind === "link") {
      return (
        <a
          key={i}
          href={p.url}
          onClick={(e) => {
            e.preventDefault();
            onOpenLink(p.url);
          }}
        >
          {p.text}
        </a>
      );
    }
    return <Fragment key={i}>{p.text}</Fragment>;
  });
}

/** The whole `PRIVACY.md`, built into the app - works offline and needs no public repository. */
function PrivacyText({ id, onOpenLink }: { id: string; onOpenLink: (url: string) => void }) {
  const { lang } = useI18n();
  const blocks = useMemo(() => privacyBlocks(privacyPolicy, lang), [lang]);
  return (
    <div id={id} className="privacy-text">
      {blocks.map((b, i) => {
        if (b.kind === "list") {
          return (
            <ul key={i}>
              {b.items.map((item, n) => (
                <li key={n}>
                  <Parts parts={item} onOpenLink={onOpenLink} />
                </li>
              ))}
            </ul>
          );
        }
        const Tag = b.kind === "heading" ? "h4" : "p";
        return (
          <Tag key={i}>
            <Parts parts={b.parts} onOpenLink={onOpenLink} />
          </Tag>
        );
      })}
    </div>
  );
}

/** Settings → "About": version, privacy in short, licences. */
export function AboutPart({ version, onOpenLink }: { version: string; onOpenLink: (url: string) => void }) {
  const { t } = useI18n();
  const noticesId = useId();
  const privacyId = useId();
  const [privacyOpen, setPrivacyOpen] = useState(false);
  // Full licence texts - loaded only when asked for (about 0.7 MB).
  const [notices, setNotices] = useState<string | null>(null);
  const [noticesFailed, setNoticesFailed] = useState(false);

  async function toggleNotices() {
    if (notices !== null) {
      setNotices(null);
      return;
    }
    try {
      const resp = await fetch("/third-party-notices.txt");
      if (!resp.ok) throw new Error(String(resp.status));
      setNotices(unwrapNotices(await resp.text()));
      setNoticesFailed(false);
    } catch {
      setNoticesFailed(true);
    }
  }

  return (
    <div className="settings-part about">
      <p>
        <strong>{t("app.name")}</strong> · {t("about.version", { version })}
      </p>
      <p>{t("about.what")}</p>

      <section className="source" aria-labelledby="privacy-title">
        <h3 id="privacy-title" className="source-title">{t("about.privacyTitle")}</h3>
        <ul className="about-list">
          <li>{t(isPhone() ? "about.p1Phone" : "about.p1")}</li>
          <li>{t(isPhone() ? "about.p2Phone" : "about.p2")}</li>
          <li>{t("about.p3")}</li>
          <li>{t("about.p4")}</li>
        </ul>
        <div>
          <button
            type="button"
            className="btn btn-ghost btn-small"
            aria-expanded={privacyOpen}
            aria-controls={privacyId}
            onClick={() => setPrivacyOpen(!privacyOpen)}
          >
            {t(privacyOpen ? "about.privacyHide" : "about.privacyFull")}
          </button>
        </div>
        <Reveal show={privacyOpen}>
          <PrivacyText id={privacyId} onOpenLink={onOpenLink} />
        </Reveal>
      </section>

      <section className="source" aria-labelledby="licenses-title">
        <h3 id="licenses-title" className="source-title">{t("about.licenses")}</h3>
        <p>{t("about.appLicense")}</p>
        <dl className="copy-list">
          {LICENSES.map(([name, license]) => (
            <div key={name}>
              <dt>{name}</dt>
              <dd>{license}</dd>
            </div>
          ))}
        </dl>
        <div className="about-buttons">
          <button
            type="button"
            className="btn btn-ghost btn-small"
            aria-expanded={notices !== null}
            aria-controls={noticesId}
            onClick={toggleNotices}
          >
            {t(notices === null ? "about.noticesShow" : "about.noticesHide")}
          </button>
        </div>
        {noticesFailed && (
          <p className="source-warn">
            <IconWarning /> {t("about.noticesFailed")}
          </p>
        )}
        {notices !== null && (
          <pre id={noticesId} className="notices" tabIndex={0} aria-label={t("about.noticesShow")}>
            {notices}
          </pre>
        )}
      </section>
    </div>
  );
}
