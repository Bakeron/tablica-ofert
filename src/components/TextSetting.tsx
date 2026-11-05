import { useEffect, useId, useState } from "react";
import { IconWarning } from "./Icons";

/**
 * A text setting saved when the field is left (not on every key). Rust checks the value. `onSave`
 * may answer with an error text: the typed text then stays in the field and the error is shown under it.
 */
export function TextSetting({
  label,
  hint,
  value,
  placeholder,
  type = "text",
  inputMode,
  onSave,
}: {
  label: string;
  hint?: string;
  value: string;
  placeholder?: string;
  type?: "text" | "email";
  inputMode?: "numeric";
  onSave: (value: string) => void | Promise<string | null | void>;
}) {
  const [draft, setDraft] = useState(value);
  const [error, setError] = useState<string | null>(null);
  const errorId = useId();
  useEffect(() => setDraft(value), [value]);
  return (
    <label className="field">
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
        maxLength={254}
        value={draft}
        placeholder={placeholder}
        onChange={(e) => {
          setDraft(e.target.value);
          setError(null);
        }}
        onBlur={async () => {
          if (draft.trim() === value) return;
          setError((await onSave(draft.trim())) || null);
        }}
      />
      {error && (
        <span id={errorId} className="source-warn" role="alert">
          <IconWarning /> {error}
        </span>
      )}
      {hint && <span className="hint">{hint}</span>}
    </label>
  );
}
