import { motionReduced } from "./presence";

/** How long after the change the height still follows: a section may need a while to load its data. */
const FOLLOW_MS = 1500;

/**
 * A window with `height: auto` jumps to the height of new content - CSS cannot animate from
 * auto to auto. Call this just before the content changes and `glide` right after it is in the
 * page (a layout effect - before it is painted, so no frame shows the new height first): the
 * window glides from the height it has to each new one (at once with reduced motion). Later
 * changes (a section loading its data) are followed for a while too.
 */
export function followHeight(box: HTMLElement, content: HTMLElement): { glide: () => void; stop: () => void } {
  const duration = parseFloat(getComputedStyle(box).getPropertyValue("--t-reveal"));
  if (motionReduced() || !duration) return { glide: () => {}, stop: () => {} };

  let shown = box.getBoundingClientRect().height;
  let contentHeight = content.offsetHeight;
  let running: Animation | null = null;

  const glide = () => {
    contentHeight = content.offsetHeight;
    // Mid-way: start from where the window is now, not from where it was going.
    if (running) shown = box.getBoundingClientRect().height;
    running?.cancel();
    const target = box.getBoundingClientRect().height;
    if (Math.abs(target - shown) < 1) return;
    // No scrollbar flashing by while the window is shorter than its content.
    box.style.overflowY = "hidden";
    running = box.animate({ height: [`${shown}px`, `${target}px`] }, { duration, easing: "ease-out" });
    running.onfinish = () => {
      box.style.overflowY = "";
      running = null;
      shown = target;
    };
  };

  const watcher = new ResizeObserver(() => {
    if (content.offsetHeight !== contentHeight) glide();
  });
  watcher.observe(content);
  const timer = setTimeout(() => watcher.disconnect(), FOLLOW_MS);
  return {
    glide,
    stop: () => {
      clearTimeout(timer);
      watcher.disconnect();
      running?.cancel();
      box.style.overflowY = "";
    },
  };
}
