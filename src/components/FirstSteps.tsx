import { useEffect, useState, type FormEvent } from "react";
import type { ProfileInput } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { IconArrowRight } from "./Icons";
import { CloseButton, Modal } from "./Modal";
import { FieldErrorsProvider, useFieldErrors } from "./FieldError";
import { ProfileFields, toForm, toInput, type Form } from "./ProfileDialog";

const STEPS: TextKey[] = ["first.s1", "first.s2", "first.s3"];

/**
 * First start: 1) the whole first profile - the same fields as the profile window, so
 * nothing is hidden for later, 2) AI provider, 3) offer sources.
 */
export function FirstSteps({
  open,
  multi,
  collect,
  onCollect,
  onSaveProfile,
  onFinish,
}: {
  open: boolean;
  /** "Several profiles" on - the profile gets a name; off - it is simply "My profile". */
  multi: boolean;
  /** "Collect offers automatically" - asked in the last step, saved at once. */
  collect: boolean;
  onCollect: (on: boolean) => void;
  onSaveProfile: (input: ProfileInput, experience: string) => Promise<boolean>;
  /** Finished or skipped - do not show again. */
  onFinish: () => void;
}) {
  const { t } = useI18n();
  const { errors, formProps } = useFieldErrors(open);
  const [step, setStep] = useState(0);
  const [form, setForm] = useState<Form>(() => toForm(null, "", "Plan A"));
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [initial] = useState(() => JSON.stringify(form));

  // The long profile form scrolls; every step starts at the top.
  useEffect(() => {
    document.getElementById("first-title")?.scrollIntoView({ block: "nearest" });
  }, [step]);

  async function next(e: FormEvent) {
    e.preventDefault();
    if (step === 0 && !saved) {
      setBusy(true);
      const named = multi ? form : { ...form, name: t("profile.defaultName") };
      const ok = await onSaveProfile(toInput(named, null), form.experience);
      setBusy(false);
      if (!ok) return;
      setSaved(true);
    }
    if (step < STEPS.length - 1) setStep(step + 1);
    else onFinish();
  }

  return (
    <Modal
      open={open}
      onClose={onFinish}
      labelledBy="first-title"
      className="modal-wide"
      dirty={!saved && JSON.stringify(form) !== initial}
    >
      <FieldErrorsProvider value={errors}>
      <form className="sheet" onSubmit={next} {...formProps}>
        <header className="sheet-head">
          <h2 id="first-title">{t("first.title")}</h2>
          <span className="first-head-side">
            <span className="first-count">{t("first.step", { n: step + 1, total: STEPS.length })}</span>
            <CloseButton onClick={onFinish} label={saved ? t("settings.close") : t("first.skip")} />
          </span>
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
            <p>{t(multi ? "first.s1Intro" : "first.s1IntroSingle")}</p>
            <ProfileFields form={form} setForm={setForm} showName={multi} descriptionPrev={null} experiencePrev={null} aiOn={false} />
          </fieldset>
        )}
        {step === 1 && <p className="first-text">{t("first.s2Text")}</p>}
        {step === 2 && (
          <>
            <fieldset className="choice">
              <legend>{t("first.collectQuestion")}</legend>
              <div className="seg-strip">
                {[true, false].map((on) => (
                  <label key={String(on)} className="seg seg-radio">
                    <input type="radio" className="sr-only" name="collect" checked={collect === on} onChange={() => onCollect(on)} />
                    {t(on ? "first.collectYes" : "first.collectNo")}
                  </label>
                ))}
              </div>
            </fieldset>
            <p className="first-text">{t(collect ? "first.s3Text" : "first.s3Own")}</p>
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
      </FieldErrorsProvider>
    </Modal>
  );
}
