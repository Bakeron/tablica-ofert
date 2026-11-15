import { useEffect, useRef, useState, type RefObject } from "react";

/** Length of every show / hide transition (the same value as `--t-reveal` in app.css). */
export const REVEAL_MS = 180;

/** The user (or the system) asked for less motion - things then appear and go at once. */
export function motionReduced(): boolean {
  return typeof document !== "undefined" && document.documentElement.dataset.motion === "reduced";
}

/**
 * Keeps a part on screen for a moment after `show` turns false, so it can play its exit
 * transition. `leaving` = on its way out (CSS: `[data-leaving]`), `mounted` = still rendered.
 */
export function usePresence(show: boolean, ms = REVEAL_MS): { mounted: boolean; leaving: boolean } {
  const [mounted, setMounted] = useState(show);
  useEffect(() => {
    if (show) {
      setMounted(true);
      return;
    }
    if (motionReduced()) {
      setMounted(false);
      return;
    }
    const timer = window.setTimeout(() => setMounted(false), ms);
    return () => window.clearTimeout(timer);
  }, [show, ms]);
  return { mounted: show || mounted, leaving: !show && mounted };
}

/** What to draw while leaving: the last content shown (the live one may already be empty). */
export function useLastShown<T>(show: boolean, value: T): T {
  const kept = useRef(value);
  if (show) kept.current = value;
  return show ? value : kept.current;
}

/** A short fade-in of `ref` whenever `key` changes (e.g. the card shows another offer); `ms` - how long. */
export function useAppearOn(ref: RefObject<HTMLElement | null>, key: unknown, ms = REVEAL_MS) {
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    const el = ref.current;
    if (!el || motionReduced() || typeof el.animate !== "function") return;
    el.animate([{ opacity: 0.25, translate: "0 6px" }, { opacity: 1, translate: "0 0" }], { duration: ms, easing: "ease-out" });
  }, [ref, key, ms]);
}
