import { useEffect, useRef, useState, type MouseEvent, type ReactNode } from "react";
import { useI18n } from "../i18n";
import { onBack } from "../lib/back";
import { useLastShown, usePresence } from "../lib/presence";
import { IconClose } from "./Icons";
import { ToastView } from "./Toast";

/** The same "X" in the top right corner of every window and message. */
export function CloseButton({ onClick, label, small = false }: { onClick: () => void; label?: string; small?: boolean }) {
  const { t } = useI18n();
  return (
    <button type="button" className={`icon-btn close-btn${small ? " icon-btn-small" : ""}`} onClick={onClick} aria-label={label ?? t("settings.close")}>
      <IconClose />
    </button>
  );
}

/**
 * Native modal <dialog>: focus stays inside, the page behind is inert. Esc and a click
 * outside the window close it - but with unsaved changes (`dirty`) they ask first, so a
 * stray click never throws away typed text. Opening and closing take a short slide and fade
 * (the window stays a moment after `open` turns false).
 */
export function Modal({
  open,
  onClose,
  labelledBy,
  className = "",
  dirty = false,
  children,
}: {
  open: boolean;
  onClose: () => void;
  labelledBy: string;
  className?: string;
  dirty?: boolean;
  children: ReactNode;
}) {
  const { t } = useI18n();
  const ref = useRef<HTMLDialogElement>(null);
  const pressedOutside = useRef(false);
  const [asking, setAsking] = useState(false);
  const { mounted, leaving } = usePresence(open);
  const ask = usePresence(open && asking);
  const content = useLastShown(open, children);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) {
      dialog.showModal();
      // showModal() focuses the first button; start where typing begins instead.
      dialog.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    }
    if (!mounted && dialog.open) dialog.close();
    if (!open) setAsking(false);
  }, [open, mounted]);

  function softClose() {
    if (dirty) setAsking(true);
    else onClose();
  }

  // Android "back" closes the window the same way as Esc (asking first about unsaved text).
  const closeRef = useRef(softClose);
  closeRef.current = softClose;
  useEffect(() => (open ? onBack(() => closeRef.current()) : undefined), [open]);

  // Only a click that starts and ends outside the window counts (not text selected
  // inside and released outside).
  const outside = (e: MouseEvent<HTMLDialogElement>) => {
    if (e.target !== e.currentTarget) return false;
    const r = e.currentTarget.getBoundingClientRect();
    return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
  };

  return (
    <dialog
      ref={ref}
      className={`modal ${className}`}
      aria-labelledby={labelledBy}
      data-leaving={leaving || undefined}
      onMouseDown={(e) => (pressedOutside.current = outside(e))}
      onClick={(e) => {
        if (pressedOutside.current && outside(e)) softClose();
        pressedOutside.current = false;
      }}
      onCancel={(e) => {
        if (e.target !== e.currentTarget) return;
        // Esc closes through the same path as "X", so the window slides away too.
        e.preventDefault();
        softClose();
      }}
      onClose={(e) => {
        // A window opened inside this one (e.g. "Tidy up") closing is not this one closing.
        if (e.target === e.currentTarget) onClose();
      }}
    >
      {ask.mounted && (
        <div className="leave-ask fade" role="alert" data-leaving={ask.leaving || undefined} inert={ask.leaving || undefined}>
          <p>{t("modal.leaveAsk")}</p>
          <button type="button" className="btn btn-ghost btn-small" onClick={() => setAsking(false)} autoFocus>
            {t("modal.stay")}
          </button>
          <button type="button" className="btn btn-danger btn-small" onClick={onClose}>
            {t("modal.leave")}
          </button>
        </div>
      )}
      {mounted && content}
      {mounted && <ToastView inWindow />}
    </dialog>
  );
}
