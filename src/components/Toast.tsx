import { createContext, useContext, type CSSProperties } from "react";
import { useI18n, type TextKey } from "../i18n";
import type { WorkProgress } from "../api/types";
import { IconWarning } from "./Icons";
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

export interface ErrorNote {
  text: string;
  detail?: string;
}

/** The app's last error - on the page it is the bar under the top bar. */
export const ErrorContext = createContext<{ error: ErrorNote | null; close: () => void }>({ error: null, close: () => {} });

/**
 * The same error inside an open window: the bar on the page is behind the window and
 * inert, so a window shows its own copy, in the corner above the message.
 */
function WindowError() {
  const { t } = useI18n();
  const { error: live, close } = useContext(ErrorContext);
  const { mounted, leaving } = usePresence(live !== null);
  const error = useLastShown(live !== null, live);
  if (!mounted || !error) return null;
  return (
    <div className="alert window-error fade" role="alert" data-leaving={leaving || undefined} inert={leaving || undefined}>
      <IconWarning />
      <div>
        <strong>{t("error.title")}: </strong>
        {error.text}
        {error.detail && (
          <details>
            <summary>{t("error.details")}</summary>
            <code>{error.detail}</code>
          </details>
        )}
      </div>
      <CloseButton small onClick={close} label={t("error.dismiss")} />
    </div>
  );
}

/** What fetching or AI is doing now (null = nothing). */
export const WorkContext = createContext<WorkProgress | null>(null);

function workText(p: WorkProgress, t: (key: TextKey, params?: Record<string, string | number>) => string): string {
  const name = (source: string | null) => t(`source.${source ?? ""}` as TextKey);
  const count = { done: p.done, total: p.total };
  switch (p.step) {
    case "wait":
      return t("work.wait", { source: name(p.source), ...count });
    case "check":
      return t("work.check", { source: name(p.source), ...count });
    case "ai":
      return t("work.ai", count);
    case "mail":
      return t("work.mail");
    case "pull":
      return t("work.pull", { model: p.source ?? "", done: p.done });
    default:
      return p.total > 1 ? t("work.sourceOf", { source: name(p.step), ...count }) : t("work.source", { source: name(p.step) });
  }
}

/**
 * The line "Now: ..." at the bottom while offers are fetched or read by AI - a long fetch
 * never looks frozen. Not read aloud at every step (it changes every second), only shown.
 */
function WorkLine() {
  const { t } = useI18n();
  const live = useContext(WorkContext);
  const { mounted, leaving } = usePresence(live !== null);
  const work = useLastShown(live !== null, live);
  if (!mounted || !work) return null;
  const share = work.total > 0 ? Math.min(1, work.done / work.total) : 0;
  return (
    <div className="work-line fade" aria-live="off" aria-label={t("work.label")} data-leaving={leaving || undefined}>
      <span className="work-now">{t("work.now")}</span>
      <span className="work-text">{workText(work, t)}</span>
      {work.total > 1 && <span className="work-bar" style={{ "--share": share } as CSSProperties} aria-hidden="true" />}
    </div>
  );
}

/**
 * The message on a small black board. It goes by itself after 10 s (held while the pointer
 * is over it or focus is inside), or earlier with "X" or Esc. Rendered on the
 * page and inside every open window - an open window makes everything behind it inert,
 * so only the copy in the topmost window can be read and clicked.
 */
export function ToastView({ inWindow = false }: { inWindow?: boolean }) {
  const { t } = useI18n();
  const { toast: live, close, hold } = useContext(ToastContext);
  const { mounted, leaving } = usePresence(live !== null);
  const toast = useLastShown(live !== null, live);
  return (
    <div className="toast-zone" aria-live="polite">
      {inWindow && <WindowError />}
      <WorkLine />
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
