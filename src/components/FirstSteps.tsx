import { useState, type FormEvent } from "react";
import type { ProfileInput } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { emptyCriteria } from "../lib/profile";
import { DescriptionField } from "./DescriptionField";
import { IconArrowRight } from "./Icons";
import { Modal } from "./Modal";

const STEPS: TextKey[] = ["first.s1", "first.s2", "first.s3"];

/** First start: 1) what you are looking for (creates the first profile), 2) AI provider, 3) offer sources. */
export function FirstSteps({
  open,
  onSaveProfile,
  onFinish,
}: {
  open: boolean;
  onSaveProfile: (input: ProfileInput, experience: string) => Promise<boolean>;
  /** Finished or skipped - do not show again. */
  onFinish: () => void;
}) {
  const { t } = useI18n();
  const [step, setStep] = useState(0);
  const [name, setName] = useState("Plan A");
  const [description, setDescription] = useState("");
  const [experience, setExperience] = useState("");
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);

  async function next(e: FormEvent) {
    e.preventDefault();
    if (step === 0 && !saved) {
      setBusy(true);
      const ok = await onSaveProfile({ id: null, name, description, criteria: emptyCriteria() }, experience);
      setBusy(false);
      if (!ok) return;
      setSaved(true);
    }
    if (step < STEPS.length - 1) setStep(step + 1);
    else onFinish();
  }

  return (
    <Modal open={open} onClose={onFinish} labelledBy="first-title" className="modal-wide">
      <form className="sheet" onSubmit={next}>
        <header className="sheet-head">
          <h2 id="first-title">{t("first.title")}</h2>
          <span className="first-count">{t("first.step", { n: step + 1 })}</span>
        </header>

        <ol className="route" aria-label={t("first.title")}>
          {STEPS.map((s, i) => (
            <li key={s} className={i === step ? "is-here" : i < step ? "is-done" : undefined} aria-current={i === step ? "step" : undefined}>
              <span className="route-no">{i + 1}</span> {t(s)}
              {i < STEPS.length - 1 && <IconArrowRight />}
            </li>
          ))}
        </ol>

        {step === 0 && (
          // Once saved, the first profile is changed in the profile window, not here.
          <fieldset className="plain" disabled={saved}>
            <p>{t("first.s1Intro")}</p>
            <label className="field field-name">
              <span>
                {t("profile.name")} <em className="req">({t("add.required")})</em>
              </span>
              <input required maxLength={40} value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            <DescriptionField value={description} onChange={setDescription} autoFocus />
            <label className="field">
              <span>{t("profile.experience")}</span>
              <textarea
                rows={3}
                maxLength={20000}
                aria-describedby="first-experience-hint"
                value={experience}
                onChange={(e) => setExperience(e.target.value)}
              />
              <span id="first-experience-hint" className="hint">
                {t("profile.experienceHint")}
              </span>
            </label>
          </fieldset>
        )}
        {step === 1 && <p className="first-text">{t("first.s2Text")}</p>}
        {step === 2 && (
          <>
            <p className="first-text">{t("first.s3Text")}</p>
            <p className="hint">{t("first.criteriaTip")}</p>
          </>
        )}

        <footer className="sheet-foot">
          {!saved && (
            <button type="button" className="btn btn-ghost sheet-foot-start" onClick={onFinish}>
              {t("first.skip")}
            </button>
          )}
          {step > 0 && (
            <button type="button" className="btn btn-ghost" onClick={() => setStep(step - 1)}>
              {t("first.back")}
            </button>
          )}
          <button type="submit" className="btn btn-sign" disabled={busy}>
            {step === STEPS.length - 1 ? t("first.done") : t("first.next")}
          </button>
        </footer>
      </form>
    </Modal>
  );
}
