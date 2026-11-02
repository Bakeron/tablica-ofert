import type { Settings } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { IconClose } from "./Icons";
import { Modal } from "./Modal";

const OPTIONS: { key: keyof Settings; label: TextKey; values: string[] }[] = [
  { key: "language", label: "settings.language", values: ["system", "pl", "en"] },
  { key: "theme", label: "settings.theme", values: ["system", "dark", "light"] },
  { key: "motion", label: "settings.motion", values: ["system", "full", "reduced"] },
];

const LANGUAGE_NAMES: Record<string, string> = { pl: "Polski", en: "English" };

export function SettingsDialog({
  open,
  settings,
  onClose,
  onChange,
}: {
  open: boolean;
  settings: Settings;
  onClose: () => void;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}) {
  const { t } = useI18n();
  return (
    <Modal open={open} onClose={onClose} labelledBy="settings-title">
      <div className="sheet">
        <header className="sheet-head">
          <h2 id="settings-title">{t("settings.title")}</h2>
          <button type="button" className="icon-btn" onClick={onClose} aria-label={t("settings.close")}>
            <IconClose />
          </button>
        </header>
        {OPTIONS.map((option) => (
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
        <footer className="sheet-foot">
          <button type="button" className="btn btn-sign" onClick={onClose}>
            {t("settings.close")}
          </button>
        </footer>
      </div>
    </Modal>
  );
}
