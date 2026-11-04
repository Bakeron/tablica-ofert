import { useId, useState } from "react";
import { useI18n } from "../i18n";
import { IconWarning } from "./Icons";

const REPO = "https://github.com/Bakeron/tablica-ofert";

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
];

/** Settings → "About": version, privacy in short, licences. */
export function AboutPart({ version, onOpenLink }: { version: string; onOpenLink: (url: string) => void }) {
  const { t } = useI18n();
  const noticesId = useId();
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
      setNotices(await resp.text());
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
          <li>{t("about.p1")}</li>
          <li>{t("about.p2")}</li>
          <li>{t("about.p3")}</li>
          <li>{t("about.p4")}</li>
        </ul>
        <div>
          <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(`${REPO}/blob/main/PRIVACY.md`)}>
            {t("about.privacyFull")}
          </button>
        </div>
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
          <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(REPO)}>
            {t("about.repo")}
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
