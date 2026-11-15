import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { useI18n, type TextKey } from "../i18n";
import { onBack } from "../lib/back";
import { useLastShown, usePresence } from "../lib/presence";

// The guide in bubbles: a yellow wayfinding sign next to each main part of the screen, with an
// arrow towards it and the part lit up (the rest of the screen dimmed). "Next" / "Skip", keys
// (Enter, arrows, Esc) and Android "back" (the step before; on the first one it closes).

export type TourStep = {
  /** CSS selectors of the part to show - everything matching that is on screen, together. */
  target: string;
  title: TextKey;
  text: TextKey;
};

/** Where the sign stands against the part it points at. */
type Side = "below" | "above" | "left" | "right" | "over";

/** Space kept from the window's edges and between the part and the sign. */
const EDGE = 16;
const GAP = 14;
/** How far the light reaches around the part. */
const PAD = 6;

type Box = { top: number; left: number; width: number; height: number };

/** The matching elements that really show (not hidden, not zero size). */
function findTargets(selectors: string): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>(selectors)].filter((el) => {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  });
}

/** The lit part: all the elements and a little around them, cut to the window. */
function litBox(els: HTMLElement[]): Box | null {
  if (els.length === 0) return null;
  const rs = els.map((el) => el.getBoundingClientRect());
  const top = Math.max(0, Math.min(...rs.map((r) => r.top)) - PAD);
  const left = Math.max(0, Math.min(...rs.map((r) => r.left)) - PAD);
  const bottom = Math.min(window.innerHeight, Math.max(...rs.map((r) => r.bottom)) + PAD);
  const right = Math.min(window.innerWidth, Math.max(...rs.map((r) => r.right)) + PAD);
  return { top, left, width: Math.max(0, right - left), height: Math.max(0, bottom - top) };
}

/** The dim over the whole window with a hole for the lit part (even-odd: the inner square is cut out). */
function dimShape(b: Box | null): string {
  if (!b) return "none";
  const [x1, y1, x2, y2] = [b.left, b.top, b.left + b.width, b.top + b.height].map((v) => `${Math.round(v)}px`);
  return `polygon(evenodd, 0 0, 100% 0, 100% 100%, 0 100%, 0 0, ${x1} ${y1}, ${x2} ${y1}, ${x2} ${y2}, ${x1} ${y2}, ${x1} ${y1})`;
}

/** The sign's place: below the part, else above, else beside it, else over its lower part. */
export function placeSign(part: Box, sign: { width: number; height: number }, view: { width: number; height: number }) {
  const clamp = (v: number, min: number, max: number) => Math.min(Math.max(v, min), Math.max(min, max));
  const centerX = part.left + part.width / 2;
  const centerY = part.top + part.height / 2;
  const x = clamp(centerX - sign.width / 2, EDGE, view.width - sign.width - EDGE);
  const y = clamp(centerY - sign.height / 2, EDGE, view.height - sign.height - EDGE);
  let side: Side;
  let left: number;
  let top: number;
  if (view.height - (part.top + part.height) >= sign.height + GAP + EDGE) {
    side = "below";
    left = x;
    top = part.top + part.height + GAP;
  } else if (part.top >= sign.height + GAP + EDGE) {
    side = "above";
    left = x;
    top = part.top - GAP - sign.height;
  } else if (part.left >= sign.width + GAP + EDGE) {
    side = "left";
    left = part.left - GAP - sign.width;
    top = y;
  } else if (view.width - (part.left + part.width) >= sign.width + GAP + EDGE) {
    side = "right";
    left = part.left + part.width + GAP;
    top = y;
  } else {
    side = "over";
    left = x;
    top = clamp(part.top + part.height - sign.height - EDGE, EDGE, view.height - sign.height - EDGE);
  }
  // The pointer aims at the middle of the part, but stays on the sign's edge.
  const arrow =
    side === "below" || side === "above" ? clamp(centerX - left, 18, sign.width - 18) : clamp(centerY - top, 18, sign.height - 18);
  return { side, left, top, arrow };
}

export function Tour({ open, steps: liveSteps, onFinish }: { open: boolean; steps: TourStep[]; onFinish: () => void }) {
  // While fading out, the last step stays as it was (the list of steps may change behind it).
  const steps = useLastShown(open, liveSteps);
  const { t } = useI18n();
  const titleId = useId();
  const textId = useId();
  const ref = useRef<HTMLDialogElement>(null);
  const signRef = useRef<HTMLDivElement>(null);
  const nextRef = useRef<HTMLButtonElement>(null);
  const [index, setIndex] = useState(0);
  const [lit, setLit] = useState<Box | null>(null);
  const [place, setPlace] = useState<ReturnType<typeof placeSign> | null>(null);
  const { mounted, leaving } = usePresence(open);
  const step = steps[Math.min(index, steps.length - 1)];
  const last = index >= steps.length - 1;

  useEffect(() => {
    if (open) setIndex(0);
  }, [open]);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!mounted && dialog.open) dialog.close();
  }, [open, mounted]);

  // The part of this step: brought into view and lit; measured again when the window changes.
  const target = step?.target;
  useLayoutEffect(() => {
    if (!open || !target) return;
    const measure = () => setLit(litBox(findTargets(target)));
    // Scrolled to only when it is out of sight - a tall list stays where the person left it.
    const first = findTargets(target)[0]?.getBoundingClientRect();
    if (first && (first.bottom <= 0 || first.top >= window.innerHeight)) findTargets(target)[0].scrollIntoView({ block: "nearest" });
    measure();
    // The part can still grow or move (the list loading, the top bar fitting its buttons).
    const watch = new ResizeObserver(measure);
    findTargets(target).forEach((el) => watch.observe(el));
    window.addEventListener("resize", measure);
    window.addEventListener("scroll", measure, true);
    return () => {
      watch.disconnect();
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", measure, true);
    };
  }, [open, target]);

  // The sign's place needs its own size, so it is set after it is drawn.
  useLayoutEffect(() => {
    const sign = signRef.current;
    if (!sign || !open) return;
    const view = { width: window.innerWidth, height: window.innerHeight };
    const part = lit ?? { top: view.height / 2, left: view.width / 2, width: 0, height: 0 };
    setPlace(placeSign(part, { width: sign.offsetWidth, height: sign.offsetHeight }, view));
  }, [lit, index, open, t]);

  // Every step starts on "Next" - Enter goes on, Tab moves to "Skip".
  // (The sign is hidden until it has its place - focus waits for that.)
  const placed = place !== null;
  useEffect(() => {
    if (open && placed) nextRef.current?.focus({ preventScroll: true });
  }, [open, index, placed]);

  const go = (by: number) => {
    const to = index + by;
    if (to < 0) return;
    if (to >= steps.length) onFinish();
    else setIndex(to);
  };

  // Android "back": the step before; on the first one the guide closes (as "Skip").
  const backRef = useRef(() => {});
  backRef.current = () => (index > 0 ? setIndex(index - 1) : onFinish());
  useEffect(() => (open ? onBack(() => backRef.current()) : undefined), [open]);

  if (!step && !mounted) return null;
  const side = place?.side ?? "below";

  return (
    <dialog
      ref={ref}
      className="tour"
      aria-labelledby={titleId}
      aria-describedby={textId}
      data-leaving={leaving || undefined}
      onCancel={(e) => {
        // Esc = "Skip".
        e.preventDefault();
        onFinish();
      }}
      onKeyDown={(e) => {
        if (e.key === "ArrowRight" || (e.key === "Enter" && !(e.target instanceof HTMLButtonElement))) {
          e.preventDefault();
          go(1);
        } else if (e.key === "ArrowLeft") {
          e.preventDefault();
          go(-1);
        } else if (e.key === "Tab") {
          // Tab goes round the sign's buttons only.
          const buttons = [...(signRef.current?.querySelectorAll<HTMLButtonElement>("button") ?? [])];
          const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
          e.preventDefault();
          buttons[(at + (e.shiftKey ? -1 : 1) + buttons.length) % buttons.length]?.focus();
        }
      }}
    >
      <div className="tour-dim" style={{ clipPath: dimShape(lit) }} aria-hidden="true" />
      {lit && <div className="tour-light" style={{ top: lit.top, left: lit.left, width: lit.width, height: lit.height }} aria-hidden="true" />}
      {step && (
        <div
          ref={signRef}
          className="tour-sign"
          data-side={side}
          style={{
            top: place?.top ?? 0,
            left: place?.left ?? 0,
            visibility: place ? undefined : "hidden",
            ["--arrow" as string]: `${place?.arrow ?? 24}px`,
          }}
        >
          <div className="tour-head">
            <span className="tour-arrow" data-side={side} aria-hidden="true">
              <svg viewBox="0 0 24 24" width="24" height="24">
                <path d="M12 3 L21 12 H15 V21 H9 V12 H3 Z" fill="currentColor" />
              </svg>
            </span>
            <h2 id={titleId} className="tour-title">
              {t(step.title)}
            </h2>
            <span className="tour-count">
              <span className="sr-only">{t("tour.stepOf", { n: index + 1, total: steps.length })}</span>
              <span aria-hidden="true">
                {index + 1}/{steps.length}
              </span>
            </span>
          </div>
          <p id={textId} className="tour-text">
            {t(step.text)}
          </p>
          <div className="tour-foot">
            {!last && (
              <button type="button" className="tour-btn tour-skip" onClick={onFinish}>
                {t("tour.skip")}
              </button>
            )}
            <button ref={nextRef} type="button" className="tour-btn tour-next" onClick={() => go(1)}>
              {last ? t("tour.done") : t("tour.next")}
            </button>
          </div>
        </div>
      )}
    </dialog>
  );
}
