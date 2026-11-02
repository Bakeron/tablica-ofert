import { useEffect, useRef, useState } from "react";

/**
 * Text on a split-flap module. When the text changes (or `fresh` is set for a new row)
 * the module flips once - about 0.3 s. Reduced motion is handled in CSS.
 */
export function Flap({ text, fresh = false, className = "" }: { text: string; fresh?: boolean; className?: string }) {
  const previous = useRef(text);
  const [flips, setFlips] = useState(fresh ? 1 : 0);

  useEffect(() => {
    if (previous.current !== text) {
      previous.current = text;
      setFlips((n) => n + 1);
    }
  }, [text]);

  return (
    <span className={`flap ${className}`}>
      <span key={flips} className={flips ? "flap-in" : undefined}>
        {text}
      </span>
    </span>
  );
}
