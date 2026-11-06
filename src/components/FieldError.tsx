import { createContext, useContext, useEffect, useRef, useState, type FormEvent } from "react";
import { useI18n } from "../i18n";
import { IconWarning } from "./Icons";

type Errors = Record<string, string>;
type Checked = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;

const ErrorsContext = createContext<Errors>({});
export const FieldErrorsProvider = ErrorsContext.Provider;

/**
 * Our own message under a field instead of the browser's bubble, which covers the field's label
 * (and on a phone often half the screen). The form keeps the browser's checks (`required`,
 * `pattern`, `min` / `max`); a field takes part when it has a `name`. Messages go away when
 * the field is fixed or the window closes.
 */
export function useFieldErrors(open: boolean) {
  const { t } = useI18n();
  const [errors, setErrors] = useState<Errors>({});
  const batch = useRef<{ first: Checked; found: Errors } | null>(null);

  useEffect(() => {
    if (!open) setErrors({});
  }, [open]);

  function message(el: Checked): string {
    const v = el.validity;
    if (v.valueMissing) return t("form.required");
    if ((v.rangeUnderflow || v.rangeOverflow) && el instanceof HTMLInputElement) return t("form.range", { min: el.min, max: el.max });
    return el.dataset.error ?? t("form.invalid");
  }

  const formProps = {
    // "invalid" fires once per wrong field during one submit; the first one gets the focus.
    onInvalidCapture: (e: FormEvent<HTMLFormElement>) => {
      const el = e.target as Checked;
      if (!el.name) return;
      e.preventDefault();
      if (!batch.current) {
        batch.current = { first: el, found: {} };
        setTimeout(() => {
          const done = batch.current;
          batch.current = null;
          if (!done) return;
          setErrors(done.found);
          done.first.focus({ preventScroll: true });
          // The middle of the screen - never under the pinned title or buttons.
          done.first.scrollIntoView({ block: "center" });
        });
      }
      batch.current.found[el.name] = message(el);
    },
    onInputCapture: (e: FormEvent<HTMLFormElement>) => {
      const el = e.target as Checked;
      if (!el.name || !el.validity) return;
      // The app's own "fill in this field" text (App.tsx) would keep the field wrong after typing.
      el.setCustomValidity("");
      if (!el.validity.valid) return;
      setErrors((now) => {
        if (!(el.name in now)) return now;
        const rest = { ...now };
        delete rest[el.name];
        return rest;
      });
    },
    onSubmitCapture: () => setErrors({}),
  };

  return { errors, formProps, field: (name: string, describedBy?: string) => fieldProps(errors, name, describedBy) };
}

/** The same props inside a part of the form (reads the form's messages from FieldErrorsProvider). */
export function useField(name: string, describedBy?: string) {
  return fieldProps(useContext(ErrorsContext), name, describedBy);
}

/** Props for a checked field: its name and, while wrong, the link to its message. */
function fieldProps(errors: Errors, name: string, describedBy?: string) {
  const wrong = name in errors;
  return {
    name,
    "aria-invalid": wrong || undefined,
    "aria-describedby": [describedBy, wrong ? `${name}-error` : undefined].filter(Boolean).join(" ") || undefined,
  };
}

/** The message itself, right under the field. */
export function FieldError({ name }: { name: string }) {
  const errors = useContext(ErrorsContext);
  if (!(name in errors)) return null;
  return (
    <span id={`${name}-error`} className="field-error">
      <IconWarning /> {errors[name]}
    </span>
  );
}
