import { useEffect, useState } from "react";

/** A text setting saved when the field is left (not on every key). Rust checks the value. */
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
  onSave: (value: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <label className="field">
      <span>{label}</span>
      <input
        type={type}
        inputMode={inputMode}
        autoComplete="off"
        spellCheck={false}
        maxLength={254}
        value={draft}
        placeholder={placeholder}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => draft.trim() !== value && onSave(draft.trim())}
      />
      {hint && <span className="hint">{hint}</span>}
    </label>
  );
}
