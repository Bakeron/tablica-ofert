import { useState } from "react";
import type { Api } from "../api";
import type { Profile, Settings } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { IconClose } from "./Icons";
import { Modal } from "./Modal";
import { AboutPart } from "./AboutPart";
import { AiSettings } from "./AiSettings";
import { DataSettings } from "./DataSettings";
import { SourcesSettings } from "./SourcesSettings";
import { SyncSettings } from "./SyncSettings";
import { isPhone } from "../lib/platform";

const OPTIONS: { key: "language" | "theme" | "motion"; label: TextKey; values: string[] }[] = [
  { key: "language", label: "settings.language", values: ["system", "pl", "en"] },
  { key: "theme", label: "settings.theme", values: ["system", "dark", "light"] },
  { key: "motion", label: "settings.motion", values: ["system", "full", "reduced"] },
];

const LANGUAGE_NAMES: Record<string, string> = { pl: "Polski", en: "English" };

export type SettingsSection = "look" | "sources" | "ai" | "sync" | "data" | "about";
/** The phone is a helper - collecting offers, AI and data tools stay on the PC. */
const SECTIONS: SettingsSection[] = isPhone() ? ["sync", "look", "about"] : ["sources", "ai", "look", "sync", "data", "about"];

const FEATURES = ["feature_dates", "feature_contacts", "feature_keys"] as const;

export function SettingsDialog({
  open,
  api,
  settings,
  profiles,
  initialSection = isPhone() ? "sync" : "sources",
  onClose,
  onChange,
  onOpenLink,
  onError,
  onToast,
  version,
  onConfirm,
  onLoaded,
}: {
  open: boolean;
  api: Api | null;
  settings: Settings;
  profiles: Profile[];
  initialSection?: SettingsSection;
  onClose: () => void;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onOpenLink: (url: string) => void;
  onError: (e: unknown) => void;
  onToast: (text: string) => void;
  version: string;
  onConfirm: (message: string, action: () => void, label?: string) => void;
  onLoaded: () => void;
}) {
  const { t } = useI18n();
  return (
    <Modal open={open} onClose={onClose} labelledBy="settings-title" className="modal-wide">
      <SettingsBody
        api={api}
        settings={settings}
        profiles={profiles}
        initialSection={initialSection}
        onClose={onClose}
        onChange={onChange}
        onOpenLink={onOpenLink}
        onError={onError}
        onToast={onToast}
        version={version}
        onConfirm={onConfirm}
        onLoaded={onLoaded}
        closeLabel={t("settings.close")}
      />
    </Modal>
  );
}

/** Inside the dialog, so the chosen section starts fresh every time the window opens. */
function SettingsBody({
  api,
  settings,
  profiles,
  initialSection,
  onClose,
  onChange,
  onOpenLink,
  onError,
  onToast,
  version,
  onConfirm,
  onLoaded,
  closeLabel,
}: {
  api: Api | null;
  settings: Settings;
  profiles: Profile[];
  initialSection: SettingsSection;
  onClose: () => void;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onOpenLink: (url: string) => void;
  onError: (e: unknown) => void;
  onToast: (text: string) => void;
  version: string;
  onConfirm: (message: string, action: () => void, label?: string) => void;
  onLoaded: () => void;
  closeLabel: string;
}) {
  const { t } = useI18n();
  const [section, setSection] = useState<SettingsSection>(initialSection);
  return (
    <div className="sheet">
      <header className="sheet-head">
        <h2 id="settings-title">{t("settings.title")}</h2>
        <button type="button" className="icon-btn" onClick={onClose} aria-label={closeLabel}>
          <IconClose />
        </button>
      </header>
      <nav className="settings-nav" aria-label={t("settings.section")}>
        {SECTIONS.map((s) => (
          <button key={s} type="button" className="seg" aria-pressed={section === s} onClick={() => setSection(s)}>
            {t(`settings.s.${s}` as TextKey)}
          </button>
        ))}
      </nav>

      {section === "look" &&
        OPTIONS.map((option) => (
          <fieldset key={option.key} className="choice">
            <legend>{t(option.label)}</legend>
            {option.values.map((value) => (
              <label key={value} className="choice-item">
                <input
                  type="radio"
                  name={option.key}
                  value={value}
                  checked={settings[option.key] === value}
                  onChange={() => onChange(option.key, value as Settings[typeof option.key])}
                />
                <span>
                  {option.key === "language" && value !== "system"
                    ? LANGUAGE_NAMES[value]
                    : t(`${option.label}.${value}` as TextKey)}
                </span>
              </label>
            ))}
          </fieldset>
        ))}

      {section === "look" && (
        <fieldset className="choice choice-column">
          <legend>{t("features.title")}</legend>
          {FEATURES.map((key) => (
            <label key={key} className="choice-item">
              <input type="checkbox" checked={settings[key] === "on"} onChange={(e) => onChange(key, e.target.checked ? "on" : "off")} />
              <span>{t(`features.${key}` as TextKey)}</span>
            </label>
          ))}
          <label className="choice-item">
            <input type="checkbox" checked={settings.sound === "on"} onChange={(e) => onChange("sound", e.target.checked ? "on" : "off")} />
            <span>{t("features.sound")}</span>
          </label>
          <p className="hint">{t("features.hint")}</p>
        </fieldset>
      )}

      {section === "data" && api && (
        <DataSettings api={api} onError={onError} onToast={onToast} onConfirm={onConfirm} onLoaded={onLoaded} />
      )}

      {section === "about" && <AboutPart version={version} onOpenLink={onOpenLink} />}

      {section === "sync" && api && (
        <SyncSettings
          api={api}
          settings={settings}
          onChange={onChange}
          onOpenLink={onOpenLink}
          onError={onError}
          onToast={onToast}
          onConfirm={onConfirm}
          onSynced={onLoaded}
        />
      )}

      {section === "sources" && api && (
        <SourcesSettings api={api} settings={settings} profiles={profiles} onChange={onChange} onOpenLink={onOpenLink} onError={onError} onToast={onToast} />
      )}

      {section === "ai" && api && (
        <AiSettings api={api} settings={settings} onChange={onChange} onOpenLink={onOpenLink} onError={onError} onToast={onToast} />
      )}

      <footer className="sheet-foot">
        <button type="button" className="btn btn-sign" onClick={onClose}>
          {closeLabel}
        </button>
      </footer>
    </div>
  );
}
