import { useI18n, type TextKey } from "../i18n";
import { isPhone } from "../lib/platform";
import { IconWarning } from "./Icons";
import { FieldError, useField } from "./FieldError";
import { PROFILE_DESCRIPTION_MAX } from "../lib/limits";

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
  const field = useField("profile-description", "describe-questions");
  return (
    <div className="describe">
      <label className="field">
        <span>
          {t("profile.description")} <em className="req">({t("add.required")})</em>
        </span>
        <textarea
          required
          rows={6}
          maxLength={PROFILE_DESCRIPTION_MAX}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          data-autofocus={autoFocus ? "" : undefined}
          {...field}
        />
        <FieldError name="profile-description" />
      </label>
      <div id="describe-questions" className="describe-questions">
        <p>{t("profile.descriptionHint")}</p>
        <ul>
          {QUESTIONS.map((q) => (
            <li key={q}>{t(q)}</li>
          ))}
        </ul>
      </div>
      <div className="warn-box describe-warn" role="note">
        <IconWarning />
        <p>
          <strong>{t("mail.warnTitle")}: </strong>
          {t(isPhone() ? "profile.noPersonalPhone" : "profile.noPersonal")}
        </p>
      </div>
    </div>
  );
}
