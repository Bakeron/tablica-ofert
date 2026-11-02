// Line pictograms in the style of airport wayfinding signs: one stroke width, square ends.
import type { SVGProps } from "react";

function Icon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="square"
      strokeLinejoin="miter"
      aria-hidden="true"
      focusable="false"
      {...props}
    />
  );
}

export const IconBriefcase = () => (
  <Icon>
    <rect x="3" y="7" width="18" height="13" />
    <path d="M9 7V4h6v3M3 12h18" />
  </Icon>
);

export const IconPlus = () => (
  <Icon>
    <path d="M12 5v14M5 12h14" />
  </Icon>
);

export const IconGear = () => (
  <Icon>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1 7 17M17 7l2.1-2.1" />
  </Icon>
);

export const IconStar = ({ filled }: { filled: boolean }) => (
  <Icon fill={filled ? "currentColor" : "none"} strokeLinejoin="round">
    <path d="m12 3 2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1L3.2 9.5l6.1-.9z" />
  </Icon>
);

export const IconTrash = () => (
  <Icon>
    <path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6" />
  </Icon>
);

export const IconArrowOut = () => (
  <Icon>
    <path d="M14 4h6v6M20 4l-9 9M18 14v6H4V6h6" />
  </Icon>
);

export const IconRestore = () => (
  <Icon>
    <path d="M4 4v6h6M4 10a8 8 0 1 1 2 6" />
  </Icon>
);

export const IconWarning = () => (
  <Icon>
    <path d="M12 3 2 21h20zM12 10v5M12 18v.5" />
  </Icon>
);

export const IconClose = () => (
  <Icon>
    <path d="M6 6l12 12M18 6 6 18" />
  </Icon>
);

export const IconPencil = () => (
  <Icon>
    <path d="M4 20h4L19 9l-4-4L4 16zM13 7l4 4" />
  </Icon>
);

/** Wayfinding arrow, as on direction signs. */
export const IconArrowRight = () => (
  <Icon>
    <path d="M4 12h15M13 6l6 6-6 6" />
  </Icon>
);
