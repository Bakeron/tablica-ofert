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

/** "Today": the sun over the horizon line. */
export const IconSun = () => (
  <Icon>
    <path d="M3 18h18M7 18a5 5 0 0 1 10 0M12 6v3M5.6 9.6l2 2M18.4 9.6l-2 2" />
  </Icon>
);

/** The archive: a box with its lid. */
export const IconArchive = () => (
  <Icon>
    <rect x="3" y="4" width="18" height="5" />
    <path d="M5 9v11h14V9M10 13h4" />
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

export const IconHome = () => (
  <Icon>
    <path d="M3 11 12 4l9 7M5 9.5V20h5v-6h4v6h5V9.5" />
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

/** Wayfinding arrow pointing left - back to the board. */
export const IconArrowLeft = () => (
  <Icon>
    <path d="M20 12H5M11 6l-6 6 6 6" />
  </Icon>
);

export const IconDownload = () => (
  <Icon>
    <path d="M12 3v12M7 10l5 5 5-5M4 20h16" />
  </Icon>
);

/** "Offers": a departures board - rows on a panel. */
export const IconBoard = () => (
  <Icon>
    <rect x="3" y="4" width="18" height="16" />
    <path d="M7 9h10M7 13h10M7 17h6" />
  </Icon>
);

/** "To check": a question mark on a sign. */
export const IconQuestion = () => (
  <Icon>
    <rect x="3" y="3" width="18" height="18" />
    <path d="M9 9.5a3 3 0 1 1 4.5 2.6c-1 .6-1.5 1.2-1.5 2.4M12 17v1" />
  </Icon>
);

/** "Show categories": a short list - three rows, each with its mark. */
export const IconList = () => (
  <Icon>
    <path d="M9 6h12M9 12h12M9 18h12M4 6h1M4 12h1M4 18h1" />
  </Icon>
);

/** "Rejected by filter": a funnel. */
export const IconFilter = () => (
  <Icon>
    <path d="M3 4h18l-7 9v6l-4 2v-8z" />
  </Icon>
);

/** "Not for me": the no-entry sign. */
export const IconNoEntry = () => (
  <Icon>
    <circle cx="12" cy="12" r="9" />
    <path d="M7 12h10" />
  </Icon>
);
