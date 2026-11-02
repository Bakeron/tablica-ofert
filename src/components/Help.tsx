import { useId, useState } from "react";
import { useI18n } from "../i18n";

/** Small "?" next to a harder field - one or two sentences of explanation on click. */
export function Help({ text }: { text: string }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <span className="help">
      <button
        type="button"
        className="help-btn"
        aria-expanded={open}
        aria-controls={id}
        aria-label={t("profile.help")}
        title={t("profile.help")}
        onClick={() => setOpen((o) => !o)}
      >
        ?
      </button>
      <span id={id} className="help-text" hidden={!open}>
        {text}
      </span>
    </span>
  );
}
