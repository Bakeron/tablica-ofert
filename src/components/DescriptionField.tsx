import { useI18n, type TextKey } from "../i18n";

const QUESTIONS: TextKey[] = ["profile.q1", "profile.q2", "profile.q3", "profile.q4", "profile.q5"];

/** "What I'm looking for" - always written by the person, with helper questions next to it. */
export function DescriptionField({
  value,
  onChange,
  autoFocus,
}: {
  value: string;
  onChange: (value: string) => void;
  autoFocus?: boolean;
}) {
  const { t } = useI18n();
  return (
    <div className="describe">
      <label className="field">
        <span>
          {t("profile.description")} <em className="req">({t("add.required")})</em>
        </span>
        <textarea
          required
          rows={6}
          maxLength={10000}
          value={value}
          aria-describedby="describe-questions"
          onChange={(e) => onChange(e.target.value)}
          data-autofocus={autoFocus ? "" : undefined}
        />
      </label>
      <div id="describe-questions" className="describe-questions">
        <p>{t("profile.descriptionHint")}</p>
        <ul>
          {QUESTIONS.map((q) => (
            <li key={q}>{t(q)}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}
