import { createContext, useContext, useId, useState, type FormEvent } from "react";
import { describeError, type SecretName } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { IconWarning } from "./Icons";
import { Reveal } from "./Reveal";

/** The app's "Are you sure?" dialog, given by the settings window - deleting a key asks first. */
export const ConfirmContext = createContext<((message: string, action: () => void, label?: string) => void) | null>(null);

/** The small "Show password" / "Show key" link beside the label of every password or key field. */
export function ShowToggle({
  controls,
  shown,
  onToggle,
  kind = "password",
}: {
  controls: string;
  shown: boolean;
  onToggle: (shown: boolean) => void;
  kind?: "password" | "key";
}) {
  const { t } = useI18n();
  const key = kind === "password" ? (shown ? "secret.hidePassword" : "secret.showPassword") : shown ? "secret.hideKey" : "secret.showKey";
  return (
    <button type="button" className="link-btn" aria-controls={controls} aria-pressed={shown} onClick={() => onToggle(!shown)}>
      {t(key)}
    </button>
  );
}

/** A key or password: typed once, saved straight to the credential store, shown only as "••••1234". */
export function SecretField({
  name,
  label,
  hint,
  onSave,
  onDelete,
  onReveal,
}: {
  name: SecretName;
  label: string;
  /** Last 4 characters of the saved value, undefined = not set. */
  hint: string | undefined;
  onSave: (name: SecretName, value: string) => Promise<boolean>;
  onDelete: (name: SecretName) => Promise<boolean>;
  /** The saved value, read from the credential store only when "Show" is clicked. */
  onReveal: (name: SecretName) => Promise<string | null>;
}) {
  const { t } = useI18n();
  const id = useId();
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [shown, setShown] = useState(false);
  // The saved value put into the field by "Show" - unchanged, it is not saved again and
  // "Hide" takes it off the screen.
  const [revealed, setRevealed] = useState<string | null>(null);

  async function toggle(show: boolean) {
    if (!show) {
      if (revealed !== null && value === revealed) setValue("");
      setRevealed(null);
      setShown(false);
      return;
    }
    setShown(true);
    if (value === "" && hint) {
      await attempt(async () => {
        const saved = await onReveal(name);
        if (saved !== null) {
          setValue(saved);
          setRevealed(saved);
        }
        return true;
      });
    }
  }
  const isPassword = name === "mail-password";
  const confirm = useContext(ConfirmContext);

  function remove() {
    const run = async () => {
      if ((await attempt(() => onDelete(name))) && revealed !== null) {
        setValue("");
        setRevealed(null);
      }
    };
    if (confirm) confirm(t("secret.deleteConfirm", { label }), () => void run(), t("secret.delete"));
    else void run();
  }
  // Shown under the field - the app's error bar is behind the settings window.
  const [error, setError] = useState<string | null>(null);

  async function attempt(action: () => Promise<boolean>): Promise<boolean> {
    setBusy(true);
    setError(null);
    try {
      return await action();
    } catch (err) {
      setError(t(describeError(err).key as TextKey));
      return false;
    } finally {
      setBusy(false);
    }
  }

  async function save(e: FormEvent) {
    e.preventDefault();
    if (!value.trim()) return;
    if (await attempt(() => onSave(name, value))) {
      setValue("");
      setRevealed(null);
      setShown(false);
    }
  }

  return (
    <form className="secret" onSubmit={save}>
      <div className="secret-head">
        <label className="field" htmlFor={id}>
          <span>{label}</span>
        </label>
        {/* Shows what is typed, or - with an empty field - the saved value from the credential store. */}
        <ShowToggle controls={id} shown={shown} onToggle={toggle} kind={isPassword ? "password" : "key"} />
      </div>
      <div className="secret-row">
        <input
          id={id}
          type={shown ? "text" : "password"}
          autoComplete="off"
          spellCheck={false}
          maxLength={512}
          value={value}
          placeholder={hint ?? ""}
          aria-describedby={`${id}-state`}
          onChange={(e) => setValue(e.target.value)}
        />
        <button type="submit" className="btn btn-sign btn-small" disabled={busy || !value.trim() || value === revealed}>
          {t("secret.save")}
        </button>
        {hint && (
          <button
            type="button"
            className="btn btn-ghost btn-small"
            disabled={busy}
            onClick={remove}
          >
            {t("secret.delete")}
          </button>
        )}
      </div>
      <span id={`${id}-state`} className="hint">
        {hint ? t("secret.set", { hint }) : t("secret.notSet")}
      </span>
      <div aria-live="polite">
        <Reveal show={error !== null}>
          {error && (
            <p className="source-warn">
              <IconWarning /> {error}
            </p>
          )}
        </Reveal>
      </div>
    </form>
  );
}
