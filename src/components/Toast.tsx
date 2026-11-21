import { createContext, useContext, useLayoutEffect, useRef, type CSSProperties } from "react";
import { useI18n, type TextKey } from "../i18n";
import type { WorkProgress } from "../api/types";
import { IconWarning } from "./Icons";
import { useLastShown, usePresence } from "../lib/presence";
import { CloseButton } from "./Modal";

export interface Toast {
  /** Set when shown - tells two messages with the same text apart. */
  id?: number;
  text: string;
  undo?: () => void;
}

export const ToastContext = createContext<{
  toast: Toast | null;
  close: () => void;
  hold: (held: boolean) => void;
  /** How long the message stays, in ms - the bar at its bottom shrinks over this time. */
  ms: number;
  held: boolean;
}>({
  toast: null,
  close: () => {},
  hold: () => {},
  ms: 10_000,
  held: false,
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

/** Settings → Profile was left with changes not saved - closing the app would lose them. */
/** Settings → Profile left with changes not saved: the reminder, its "Open profile" and its "X". */
export const ProfileDraftContext = createContext<{ on: boolean; open: () => void; dismiss: () => void }>({ on: false, open: () => {}, dismiss: () => {} });

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
    case "ai-wait":
      return t("work.aiWait", { name: t(`ai.p.${p.source ?? ""}` as TextKey), ...count });
    case "mail":
      return t("work.mail");
    case "pull":
      return t("work.pull", { model: p.source ?? "", done: p.done });
    case "ollama-download":
      return t("work.ollamaDownload", { done: p.done });
    case "ollama-run":
      return t("work.ollamaRun");
    default:
      return p.total > 1 ? t("work.sourceOf", { source: name(p.step), ...count }) : t("work.source", { source: name(p.step) });
  }
}

/** Steps whose `done` is a percent ready; the others name the item in work, not finished yet. */
const PERCENT_STEPS = ["pull", "ollama-download"];

/** How far the step is, 0-1; `null` = cannot be told (one request, one mailbox). */
function stepShare(p: WorkProgress): number | null {
  if (p.total <= 1) return null;
  const ready = PERCENT_STEPS.includes(p.step) ? p.done : p.done - 1;
  return Math.min(1, Math.max(0, ready / p.total));
}

/**
 * The line "Now: ..." at the bottom while offers are fetched or read by AI - a long fetch
 * never looks frozen, and the bar under it says how much of the step is left. Not read aloud
 * at every step (it changes every second), only shown.
 */
function WorkLine() {
  const { t } = useI18n();
  const live = useContext(WorkContext);
  const { mounted, leaving } = usePresence(live !== null);
  const work = useLastShown(live !== null, live);
  if (!mounted || !work) return null;
  const share = stepShare(work);
  return (
    <div className="work-line fade" aria-live="off" aria-label={t("work.label")} data-leaving={leaving || undefined}>
      {work.part > 0 && work.parts > 0 && (
        <span className="work-part">{t("work.part", { part: Math.min(work.part, work.parts), parts: work.parts })}</span>
      )}
      <span className="work-now">{t("work.now")}</span>
      <span className="work-text">{workText(work, t)}</span>
      <span className="work-bar" data-unknown={share === null || undefined} style={{ "--share": share ?? 0 } as CSSProperties} aria-hidden="true" />
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
  const { toast: live, close, hold, ms, held } = useContext(ToastContext);
  const { mounted, leaving } = usePresence(live !== null);
  const toast = useLastShown(live !== null, live);
  const draft = useContext(ProfileDraftContext);
  const reminder = usePresence(draft.on);
  return (
    <div className="toast-zone" aria-live="polite">
      {inWindow && <WindowError />}
      {/* Stays until the profile is saved or its "X" is clicked - not a message that goes by itself. */}
      {reminder.mounted && (
        <div className="toast draft-note fade" data-leaving={reminder.leaving || undefined} inert={reminder.leaving || undefined}>
          <p className="toast-text">
            <IconWarning /> {t("profile.saveReminder")}
          </p>
          <button type="button" className="btn btn-ghost" onClick={draft.open}>
            {t("profile.openToSave")}
          </button>
          <CloseButton small onClick={draft.dismiss} label={t("profile.reminderDismiss")} />
        </div>
      )}
      <WorkLine />
      {mounted && toast && (
        <div
          // A new message replaces the old one with its own short entrance.
          key={toast.id}
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
          <ToastBar held={held} ms={ms} />
        </div>
      )}
    </div>
  );
}

/**
 * Time left: shrinks to nothing as the message goes. Held - it glides back to full from
 * where it was (not a jump), and on release it starts from full again, like the timer.
 */
function ToastBar({ held, ms }: { held: boolean; ms: number }) {
  const ref = useRef<HTMLSpanElement>(null);
  // When the current shrinking started (0 = not yet) - tells how much was left at the hold.
  const start = useRef(0);
  const grow = useRef<Animation | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (!held) {
      grow.current?.cancel();
      grow.current = null;
      start.current = performance.now();
      return;
    }
    if (!start.current) return;
    const left = Math.max(0, 1 - (performance.now() - start.current) / ms);
    grow.current =
      el.animate?.([{ transform: `scaleX(${left})` }, { transform: "scaleX(1)" }], { duration: 450, easing: "ease-out" }) ?? null;
  }, [held, ms]);
  return (
    <span
      ref={ref}
      className="toast-bar"
      data-held={held || undefined}
      style={{ "--toast-ms": `${ms}ms` } as CSSProperties}
      aria-hidden="true"
    />
  );
}
