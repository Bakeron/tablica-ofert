import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Api } from "../api";
import { TEXT_SIZES, type CareerIdea, type Criteria, type Profile, type ProfileInput, type Settings } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { CloseButton, Modal } from "./Modal";
import { AboutPart } from "./AboutPart";
import { AiSettings } from "./AiSettings";
import { DataSettings } from "./DataSettings";
import { ProfileSettings } from "./ProfileDialog";
import { SourcesSettings } from "./SourcesSettings";
import { SyncSettings } from "./SyncSettings";
import { ConfirmContext } from "./SecretField";
import { askNotifications, isPhone, notificationsAllowed, openAppSettings } from "../lib/platform";
import { settingsMemory } from "../lib/settingsMemory";
import { followHeight } from "../lib/smoothHeight";

const OPTIONS: { key: "language" | "theme" | "motion" | "text_size"; label: TextKey; values: readonly string[]; hint?: TextKey }[] = [
  { key: "language", label: "settings.language", values: ["system", "pl", "en"] },
  { key: "theme", label: "settings.theme", values: ["system", "dark", "light"] },
  { key: "motion", label: "settings.motion", values: ["system", "full", "reduced"] },
  // Keyboard shortcuts only on the PC.
  { key: "text_size", label: "settings.text_size", values: TEXT_SIZES, hint: isPhone() ? undefined : "settings.text_size.keys" },
];

/** How long offers stay fresh and how many "Today" shows (both travel with sync). */
const LIST_OPTIONS: { key: "fresh_days" | "today_count"; label: TextKey; unit: TextKey; values: readonly string[]; hint: TextKey }[] = [
  { key: "fresh_days", label: "list.freshDays", unit: "list.days", values: ["7", "10", "14", "21", "30"], hint: "list.freshHint" },
  { key: "today_count", label: "list.todayCount", unit: "list.offers", values: ["10", "15", "20", "30"], hint: "list.todayHint" },
];

const LANGUAGE_NAMES: Record<string, string> = { pl: "Polski", en: "English" };

export type SettingsSection = "profile" | "look" | "sources" | "ai" | "sync" | "data" | "about";
/** The phone is a helper - collecting offers, AI and data tools stay on the PC. */
const BASE_SECTIONS: SettingsSection[] = isPhone() ? ["sync", "look", "about"] : ["sources", "ai", "look", "sync", "data", "about"];

/** One profile ("Several profiles" off): its part comes first, on the PC and on the phone - and
 *  the window opens on it. With several profiles they are edited above the offer list. */
function sectionsFor(settings: Settings): SettingsSection[] {
  return settings.multi_profile === "on" ? BASE_SECTIONS : ["profile", ...BASE_SECTIONS];
}

/** What Settings → Profile needs - the same as the profile window. */
export interface ProfilePartProps {
  experience: string;
  experiencePrev: string | null;
  aiOn: boolean;
  onSuggest: (description: string) => Promise<Criteria | null>;
  onCareers: (description: string, experience: string) => Promise<CareerIdea[] | null>;
  onTidy: (text: string) => Promise<string | null>;
  onSave: (input: ProfileInput, experience: string) => Promise<boolean>;
  /** Reads the work experience (shared by all profiles) before the part shows it. */
  onLoadExperience: () => void;
  /** Settings → Profile left with changes not saved (true) - the app reminds to save them. */
  onDraft: (left: boolean) => void;
}

/** How long the window may still grow into the remembered place (sections load their data). */
const SCROLL_BACK_MS = 1500;

// Keyboard shortcuts make no sense on the phone.
const FEATURES = isPhone() ? (["feature_dates", "feature_contacts"] as const) : (["feature_dates", "feature_contacts", "feature_keys"] as const);

export function SettingsDialog({
  open,
  api,
  settings,
  profiles,
  profilePart,
  initialSection,
  onClose,
  onChange,
  onOpenLink,
  onError,
  onToast,
  onNotificationsAllowed,
  version,
  onConfirm,
  onLoaded,
}: {
  open: boolean;
  api: Api | null;
  settings: Settings;
  profiles: Profile[];
  profilePart: ProfilePartProps;
  /** Opened from a place that needs this section; without it the window comes back as it was left. */
  initialSection?: SettingsSection;
  onClose: () => void;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void | Promise<boolean>;
  onOpenLink: (url: string) => void;
  onError: (e: unknown) => void;
  onToast: (text: string) => void;
  onNotificationsAllowed: () => void;
  version: string;
  onConfirm: (message: string, action: () => void, label?: string) => void;
  onLoaded: () => void;
}) {
  const { t } = useI18n();
  return (
    <Modal open={open} onClose={onClose} labelledBy="settings-title" className="modal-wide modal-settings">
      <SettingsBody
        api={api}
        settings={settings}
        profiles={profiles}
        profilePart={profilePart}
        initialSection={initialSection}
        onClose={onClose}
        onChange={onChange}
        onOpenLink={onOpenLink}
        onError={onError}
        onToast={onToast}
        onNotificationsAllowed={onNotificationsAllowed}
        version={version}
        onConfirm={onConfirm}
        onLoaded={onLoaded}
        closeLabel={t("settings.close")}
      />
    </Modal>
  );
}

/** Inside the dialog: `settingsMemory` brings back the section and the scroll on every opening. */
function SettingsBody({
  api,
  settings,
  profiles,
  profilePart,
  initialSection,
  onClose,
  onChange,
  onOpenLink,
  onError,
  onToast,
  onNotificationsAllowed,
  version,
  onConfirm,
  onLoaded,
  closeLabel,
}: {
  api: Api | null;
  settings: Settings;
  profiles: Profile[];
  profilePart: ProfilePartProps;
  initialSection?: SettingsSection;
  onClose: () => void;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void | Promise<boolean>;
  onOpenLink: (url: string) => void;
  onError: (e: unknown) => void;
  onToast: (text: string) => void;
  onNotificationsAllowed: () => void;
  version: string;
  onConfirm: (message: string, action: () => void, label?: string) => void;
  onLoaded: () => void;
  closeLabel: string;
}) {
  const { t } = useI18n();
  const sections = sectionsFor(settings);
  const [chosen, setSection] = useState<SettingsSection>(initialSection ?? settingsMemory.section ?? sections[0]);
  // "Several profiles" turned on (here or by sync): the Profile part is gone - the first one shows.
  const section = sections.includes(chosen) ? chosen : sections[0];
  const sheet = useRef<HTMLDivElement>(null);
  const following = useRef<ReturnType<typeof followHeight> | null>(null);

  useEffect(() => {
    settingsMemory.section = section;
  }, [section]);

  useEffect(() => () => following.current?.stop(), []);

  // The new section is in the page but not painted yet: the window starts gliding from its height.
  useLayoutEffect(() => following.current?.glide(), [section]);

  function openSection(next: SettingsSection) {
    const box = sheet.current?.closest("dialog");
    if (sheet.current && box && next !== section) {
      following.current?.stop();
      following.current = followHeight(box, sheet.current);
    }
    setSection(next);
  }

  // Layout effect: its clean-up runs while the window is still there, so closing is not read as scrolling.
  useLayoutEffect(() => {
    const content = sheet.current;
    const box = content?.closest("dialog");
    if (!content || !box) return;
    const top = initialSection ? 0 : settingsMemory.scroll;
    const backToPlace = new ResizeObserver(() => {
      if (box.scrollTop < top) box.scrollTop = top;
    });
    const stop = () => backToPlace.disconnect();
    const save = () => {
      settingsMemory.scroll = box.scrollTop;
    };
    backToPlace.observe(content);
    const timer = setTimeout(stop, SCROLL_BACK_MS);
    // The user moved first - their place wins.
    box.addEventListener("wheel", stop, { once: true, passive: true });
    box.addEventListener("pointerdown", stop, { once: true });
    box.addEventListener("keydown", stop, { once: true });
    box.addEventListener("scroll", save, { passive: true });
    return () => {
      stop();
      clearTimeout(timer);
      box.removeEventListener("wheel", stop);
      box.removeEventListener("pointerdown", stop);
      box.removeEventListener("keydown", stop);
      box.removeEventListener("scroll", save);
    };
  }, [initialSection]);

  return (
    <ConfirmContext.Provider value={onConfirm}>
    <div className="sheet" ref={sheet}>
      {/* Title, "X" and the sections stay at the top while a long section scrolls. */}
      <div className="settings-top">
        <header className="sheet-head">
          <h2 id="settings-title">{t("settings.title")}</h2>
          <CloseButton onClick={onClose} label={closeLabel} />
        </header>
        <nav className="settings-nav" aria-label={t("settings.section")}>
          {sections.map((s) => (
            <button key={s} type="button" className="seg" aria-pressed={section === s} onClick={() => openSection(s)}>
              {t(`settings.s.${s}` as TextKey)}
            </button>
          ))}
        </nav>
      </div>

      {section === "look" &&
        OPTIONS.map((option) => (
          <fieldset key={option.key} className="choice">
            <legend>{t(option.label)}</legend>
            <div className="seg-strip">
              {option.values.map((value) => (
                <label key={value} className="seg seg-radio">
                  <input
                    type="radio"
                    className="sr-only"
                    name={option.key}
                    value={value}
                    checked={settings[option.key] === value}
                    onChange={() => onChange(option.key, value as Settings[typeof option.key])}
                  />
                  {option.key === "language" && value !== "system"
                    ? LANGUAGE_NAMES[value]
                    : t(`${option.label}.${value}` as TextKey)}
                </label>
              ))}
            </div>
            {option.hint && <p className="hint">{t(option.hint)}</p>}
          </fieldset>
        ))}

      {section === "look" && (
        <fieldset className="choice choice-column">
          <legend>{t("multi.title")}</legend>
          <label className="choice-item">
            <input
              type="checkbox"
              checked={settings.multi_profile === "on"}
              onChange={(e) => onChange("multi_profile", e.target.checked ? "on" : "off")}
            />
            <span>{t("multi.label")}</span>
          </label>
          <p className="hint">{t("multi.hint")}</p>
        </fieldset>
      )}

      {section === "profile" && <ProfilePart profile={profiles[0] ?? null} part={profilePart} />}

      {section === "look" && (
        <fieldset className="choice choice-column">
          <legend>{t("list.title")}</legend>
          {LIST_OPTIONS.map((option) => (
            <div key={option.key} className="list-option">
              <span id={`${option.key}-label`}>{t(option.label)}</span>
              <div className="seg-strip" role="radiogroup" aria-labelledby={`${option.key}-label`}>
                {option.values.map((value) => (
                  <label key={value} className="seg seg-radio">
                    <input
                      type="radio"
                      className="sr-only"
                      name={option.key}
                      value={value}
                      checked={settings[option.key] === value}
                      onChange={() => onChange(option.key, value as Settings[typeof option.key])}
                    />
                    {t(option.unit, { count: value })}
                  </label>
                ))}
              </div>
              <p className="hint">{t(option.hint)}</p>
            </div>
          ))}
        </fieldset>
      )}

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
          <label className="choice-item">
            <input type="checkbox" checked={settings.tilt === "on"} onChange={(e) => onChange("tilt", e.target.checked ? "on" : "off")} />
            <span>{t("features.tilt")}</span>
          </label>
          {/* On the PC this sits with the sources; the phone has no such section. */}
          {isPhone() && (
            <PhoneReminders
              on={settings.notify_reminders === "on"}
              onChange={(on) => onChange("notify_reminders", on ? "on" : "off")}
              onAllowed={onNotificationsAllowed}
            />
          )}
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
        <AiSettings api={api} settings={settings} onChange={onChange} onOpenLink={onOpenLink} onError={onError} onToast={onToast} onRated={onLoaded} />
      )}
    </div>
    </ConfirmContext.Provider>
  );
}

/** Settings → Profile: the work experience is read first (it is shared by every profile). */
function ProfilePart({ profile, part }: { profile: Profile | null; part: ProfilePartProps }) {
  const load = useRef(part.onLoadExperience);
  useEffect(() => load.current(), []);
  return (
    <ProfileSettings
      profile={profile}
      experience={part.experience}
      experiencePrev={part.experiencePrev}
      aiOn={part.aiOn}
      onSuggest={part.onSuggest}
      onCareers={part.onCareers}
      onTidy={part.onTidy}
      onSave={part.onSave}
      onDraft={part.onDraft}
    />
  );
}

/** The phone's reminders: the box ticked is not enough, Android has to allow notifications too. */
function PhoneReminders({ on, onChange, onAllowed }: { on: boolean; onChange: (on: boolean) => void; onAllowed: () => void }) {
  const { t } = useI18n();
  const [allowed, setAllowed] = useState(true);
  const seen = useRef(true);
  const report = useRef(onAllowed);
  report.current = onAllowed;

  const show = (ok: boolean) => {
    if (ok && !seen.current) report.current();
    seen.current = ok;
    setAllowed(ok);
  };

  // Coming back from Android's settings counts as a new answer.
  useEffect(() => {
    const check = () => {
      if (document.visibilityState === "visible") notificationsAllowed().then(show, () => {});
    };
    check();
    document.addEventListener("visibilitychange", check);
    return () => document.removeEventListener("visibilitychange", check);
  }, []);

  return (
    <>
      <label className="choice-item">
        <input
          type="checkbox"
          checked={on}
          onChange={(e) => {
            onChange(e.target.checked);
            if (e.target.checked) askNotifications().then(show, () => {});
          }}
        />
        <span>{t("bg.remindersPhone")}</span>
      </label>
      <div className={on && !allowed ? "source-warn notify-state" : "notify-state"}>
        <p>{t(allowed ? "bg.remindersOn" : "bg.remindersOff")}</p>
        <button
          type="button"
          className="btn btn-ghost btn-small"
          onClick={async () => {
            // An app cannot take the permission back by itself - only Android's settings can.
            if (allowed) return openAppSettings().catch(() => {});
            // After a second "no" Android stops asking - only its settings can change that.
            const ok = await askNotifications().catch(() => false);
            show(ok);
            if (!ok) await openAppSettings().catch(() => {});
          }}
        >
          {t(allowed ? "bg.remindersDisable" : "bg.remindersEnable")}
        </button>
      </div>
    </>
  );
}
