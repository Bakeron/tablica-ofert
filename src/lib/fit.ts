import { useLayoutEffect, type RefObject } from "react";

/**
 * Picks the first layout (a set of classes) in which the element's content fits on one line.
 * Measured, not guessed with media queries: the needed width depends on the language, the
 * numbers in the tabs and the chosen text size. Children must not wrap or shrink, so too much
 * content shows up as `scrollWidth > clientWidth`. A layout that wraps (e.g. the tabs in their own
 * row) also fits only when all other children (without `data-own-row`) stay in one row.
 */
export function fitLevel(el: HTMLElement, levels: string[]): string {
  const all = levels.flatMap((l) => l.split(" ")).filter(Boolean);
  let chosen = levels[levels.length - 1];
  for (const level of levels) {
    el.classList.remove(...all);
    if (level) el.classList.add(...level.split(" "));
    if (fits(el)) {
      chosen = level;
      break;
    }
  }
  el.classList.remove(...all);
  if (chosen) el.classList.add(...chosen.split(" "));
  // Classes are set straight on the element (React leaves a constant className alone).
  return chosen;
}

function fits(el: HTMLElement): boolean {
  if (el.scrollWidth > el.clientWidth + 1) return false;
  const tops = [...el.children]
    .filter((c) => c instanceof HTMLElement && !c.hasAttribute("data-own-row") && c.offsetParent !== null)
    .map((c) => (c as HTMLElement).getBoundingClientRect());
  // Same row = vertically overlapping boxes (items are centred, so their tops differ a little).
  return tops.every((r) => r.top < tops[0].bottom && r.bottom > tops[0].top);
}

/** Keeps `fitLevel` up to date when the window, the fonts or the text inside change. */
export function useFitLevels(ref: RefObject<HTMLElement | null>, levels: string[]) {
  const key = levels.join("|");
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const list = key.split("|");
    let frame = 0;
    // While hidden (the card's own screen) nothing can be measured - the last layout stays.
    const fit = () => {
      if (el.clientWidth > 0) fitLevel(el, list);
    };
    // Changes caused by `fit` itself are handled in the next frame, never in a loop.
    const later = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(fit);
    };
    fit();
    const resize = new ResizeObserver(later);
    resize.observe(el);
    const text = new MutationObserver(later);
    text.observe(el, { subtree: true, childList: true, characterData: true });
    window.addEventListener("resize", later);
    document.fonts?.ready.then(later).catch(() => {});
    return () => {
      cancelAnimationFrame(frame);
      resize.disconnect();
      text.disconnect();
      window.removeEventListener("resize", later);
    };
  }, [ref, key]);
}

/** The list and the card are separate screens (phone, tablet) - the same step as `.split` in app.css. */
export function isNarrowLayout(): boolean {
  const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  const width = document.querySelector(".app")?.clientWidth ?? window.innerWidth;
  return width <= 58 * rem;
}
