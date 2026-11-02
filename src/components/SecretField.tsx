import { useId, useState, type FormEvent } from "react";
import type { SecretName } from "../api/types";
import { useI18n } from "../i18n";

/** A key or password: typed once, saved straight to the credential store, shown only as "••••1234". */
export function SecretField({
  name,
  label,
  hint,
  onSave,
  onDelete,
}: {
  name: SecretName;
  label: string;
  /** Last 4 characters of the saved value, undefined = not set. */
  hint: string | undefined;
  onSave: (name: SecretName, value: string) => Promise<boolean>;
  onDelete: (name: SecretName) => Promise<boolean>;
}) {
  const { t } = useI18n();
  const id = useId();
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);

  async function save(e: FormEvent) {
    e.preventDefault();
    if (!value.trim()) return;
    setBusy(true);
    if (await onSave(name, value)) setValue("");
    setBusy(false);
  }

  return (
    <form className="secret" onSubmit={save}>
      <label className="field" htmlFor={id}>
        <span>{label}</span>
      </label>
      <div className="secret-row">
        <input
          id={id}
          type="password"
          autoComplete="off"
          spellCheck={false}
          maxLength={512}
          value={value}
          placeholder={hint ?? ""}
          aria-describedby={`${id}-state`}
          onChange={(e) => setValue(e.target.value)}
        />
        <button type="submit" className="btn btn-sign btn-small" disabled={busy || !value.trim()}>
          {t("secret.save")}
        </button>
        {hint && (
          <button
            type="button"
            className="btn btn-ghost btn-small"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              await onDelete(name);
              setBusy(false);
            }}
          >
            {t("secret.delete")}
          </button>
        )}
      </div>
      <span id={`${id}-state`} className="hint">
        {hint ? t("secret.set", { hint }) : t("secret.notSet")}
      </span>
    </form>
  );
}
