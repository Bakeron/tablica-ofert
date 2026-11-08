import { useEffect, useId, useLayoutEffect, useRef } from "react";
import { useI18n } from "../i18n";
import { onBack } from "../lib/back";

/**
 * Small "?" next to a harder field - a tooltip on hover or keyboard focus (tap on a phone).
 * The tooltip is a popover in the top layer, anchored to the button with CSS anchor positioning,
 * so it never moves the layout and is not cut off by the scrolling window.
 */
export function Help({ text }: { text: string }) {
  const { t } = useI18n();
  const id = useId();
  const btn = useRef<HTMLButtonElement>(null);
  const tip = useRef<HTMLSpanElement>(null);
  // Clicked (or tapped) open - stays until clicked again or focus leaves.
  const pinned = useRef(false);

  useLayoutEffect(() => {
    const anchor = `--help-${id.replace(/[^a-zA-Z0-9]/g, "")}`;
    btn.current?.style.setProperty("anchor-name", anchor);
    tip.current?.style.setProperty("position-anchor", anchor);
  }, [id]);

  // Android "back" closes an open tooltip first, not the card or window under it.
  useEffect(() => {
    const el = tip.current;
    if (!el) return;
    let stop: (() => void) | null = null;
    const onToggle = (e: Event) => {
      stop?.();
      stop = (e as ToggleEvent).newState === "open" ? onBack(() => hide(true)) : null;
    };
    el.addEventListener("toggle", onToggle);
    return () => {
      el.removeEventListener("toggle", onToggle);
      stop?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const isOpen = () => tip.current?.matches(":popover-open") ?? false;
  const show = () => {
    if (!isOpen()) tip.current?.showPopover();
  };
  const hide = (force = false) => {
    if (force) pinned.current = false;
    if (!pinned.current && isOpen()) tip.current?.hidePopover();
  };

  return (
    <span className="help">
      <button
        ref={btn}
        type="button"
        className="help-btn"
        aria-label={t("profile.help")}
        aria-describedby={id}
        onMouseEnter={show}
        onMouseLeave={() => hide()}
        onFocus={show}
        onBlur={() => hide(true)}
        onClick={() => {
          pinned.current = !pinned.current;
          if (pinned.current) show();
          else hide(true);
        }}
        onKeyDown={(e) => {
          // Esc closes the tooltip first, not the whole window.
          if (e.key === "Escape" && isOpen()) {
            e.preventDefault();
            e.stopPropagation();
            hide(true);
          }
        }}
      >
        ?
      </button>
      <span ref={tip} id={id} role="tooltip" popover="manual" className="help-tip">
        {text}
      </span>
    </span>
  );
}
