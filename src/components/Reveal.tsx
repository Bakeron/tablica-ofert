import type { ReactNode } from "react";
import { useLastShown, usePresence } from "../lib/presence";

/**
 * A part that unfolds when shown and folds away when hidden (height and fade, about 0.18 s;
 * at once with reduced motion). Use it for anything that appears on a click - forms, panels,
 * bars - instead of a bare `{show && ...}`, so nothing jumps in or out.
 */
export function Reveal({ show, className, children }: { show: boolean; className?: string; children: ReactNode }) {
  const { mounted, leaving } = usePresence(show);
  const content = useLastShown(show, children);
  if (!mounted) return null;
  return (
    <div className={className ? `reveal ${className}` : "reveal"} data-leaving={leaving || undefined} inert={leaving || undefined}>
      <div className="reveal-inner">{content}</div>
    </div>
  );
}
