import { useI18n } from "../i18n";

const REPO = "https://github.com/Bakeron/tablica-ofert";

/** Main libraries and fonts with their licences (full texts are in their repositories). */
const LICENSES: [string, string][] = [
  ["Tauri", "MIT / Apache-2.0"],
  ["React", "MIT"],
  ["rusqlite", "MIT"],
  ["SQLCipher", "BSD-3-Clause"],
  ["OpenSSL", "Apache-2.0"],
  ["reqwest, tokio, serde, time", "MIT / Apache-2.0"],
  ["imap, native-tls, mail-parser", "MIT / Apache-2.0"],
  ["keyring-core", "MIT / Apache-2.0"],
  ["Barlow Condensed, Atkinson Hyperlegible Next", "SIL Open Font License 1.1"],
];

/** Settings → "About": version, privacy in short, licences. */
export function AboutPart({ version, onOpenLink }: { version: string; onOpenLink: (url: string) => void }) {
  const { t } = useI18n();
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
        <div>
          <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(REPO)}>
            {t("about.repo")}
          </button>
        </div>
      </section>
    </div>
  );
}
