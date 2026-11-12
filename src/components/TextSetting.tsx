import { useEffect, useId, useRef, useState } from "react";
import { useI18n } from "../i18n";
import { IconWarning } from "./Icons";

/**
 * A text setting saved when the field is left (not on every key). Rust checks the value. `onSave`
 * may answer with an error text: the typed text then stays in the field and the error is shown under it.
 * `false` = not saved (the app shows why); otherwise a short "Saved" appears under the field.
 */
export function TextSetting({
  label,
  maxLength,
  hint,
  value,
  placeholder,
  type = "text",
  inputMode,
  onSave,
}: {
  label: string;
  maxLength: number;
  hint?: string;
  value: string;
  placeholder?: string;
  type?: "text" | "email";
  inputMode?: "numeric" | "decimal";
  onSave: (value: string) => void | boolean | Promise<string | boolean | null | void>;
}) {
  const { t } = useI18n();
  const [draft, setDraft] = useState(value);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  const errorId = useId();
  useEffect(() => setDraft(value), [value]);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return (
    <label className="field field-text">
      <span>{label}</span>
      <input
        type={type}
        inputMode={inputMode}
        autoComplete="off"
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        aria-invalid={error ? true : undefined}
        aria-describedby={error ? errorId : undefined}
        maxLength={maxLength}
        value={draft}
        placeholder={placeholder}
        onChange={(e) => {
          setDraft(e.target.value);
          setError(null);
        }}
        onBlur={async () => {
          if (draft.trim() === value) return;
          setSaved(false);
          const answer = await onSave(draft.trim());
          if (typeof answer === "string" && answer) {
            setError(answer);
          } else if (answer !== false) {
            setSaved(true);
            window.clearTimeout(timer.current);
            timer.current = window.setTimeout(() => setSaved(false), 3000);
          }
        }}
      />
      <span className="field-saved" role="status">
        {saved && t("settings.saved")}
      </span>
      {error && (
        <span id={errorId} className="source-warn" role="alert">
          <IconWarning /> {error}
        </span>
      )}
      {hint && <span className="hint">{hint}</span>}
    </label>
  );
}
