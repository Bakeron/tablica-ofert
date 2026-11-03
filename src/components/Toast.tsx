import { createContext, useContext } from "react";
import { useI18n } from "../i18n";
import { useLastShown, usePresence } from "../lib/presence";
import { CloseButton } from "./Modal";

export interface Toast {
  text: string;
  undo?: () => void;
}

export const ToastContext = createContext<{ toast: Toast | null; close: () => void; hold: (held: boolean) => void }>({
  toast: null,
  close: () => {},
  hold: () => {},
});

/**
 * The message on a small black board. It goes by itself after 10 s (held while the pointer
 * is over it or focus is inside), or earlier with "X" or Esc. Rendered on the
 * page and inside every open window - an open window makes everything behind it inert,
 * so only the copy in the topmost window can be read and clicked.
 */
export function ToastView() {
  const { t } = useI18n();
  const { toast: live, close, hold } = useContext(ToastContext);
  const { mounted, leaving } = usePresence(live !== null);
  const toast = useLastShown(live !== null, live);
  return (
    <div className="toast-zone" aria-live="polite">
      {mounted && toast && (
        <div
          // A new message replaces the old one with its own short entrance.
          key={toast.text}
          className="toast fade"
          data-leaving={leaving || undefined}
          inert={leaving || undefined}
          onMouseEnter={() => hold(true)}
          onMouseLeave={() => hold(false)}
          onFocus={() => hold(true)}
          onBlur={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget)) hold(false);
          }}
          onKeyDown={(e) => {
            if (e.key !== "Escape") return;
            // Only the message closes, not the window around it.
            e.preventDefault();
            e.stopPropagation();
            close();
          }}
        >
          <span className="toast-text">{toast.text}</span>
          {toast.undo && (
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => {
                toast.undo?.();
                close();
              }}
            >
              {t("trash.undo")}
            </button>
          )}
          <CloseButton small onClick={close} />
        </div>
      )}
    </div>
  );
}
