// Paths are the rail glyphs and three-node mark from the supplied design export
// (reference/Synapse — telas.html). They are reproduced, not redrawn.
import type { ReactNode } from "react";
import type { RouteId } from "../app/routes";

export function SynapseMark() {
  return (
    <svg className="mark" viewBox="0 0 22 22" aria-hidden="true">
      <path d="M5 15 L11 5 L18 14" />
      <circle cx="5" cy="15" r="2.4" />
      <circle cx="11" cy="5" r="2.4" />
      <circle cx="18" cy="14" r="2.4" />
    </svg>
  );
}

const glyph = (children: ReactNode) => (
  <svg viewBox="0 0 20 20" aria-hidden="true">
    {children}
  </svg>
);

export const RAIL_ICONS: Record<RouteId, ReactNode> = {
  home: glyph(<path d="M3.5 9.5 10 4l6.5 5.5V16h-13zM8 16v-4h4v4" />),
  projects: glyph(
    <path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H8l1.5 1.5h6A1.5 1.5 0 0 1 17 8v6.5a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 14.5z" />,
  ),
  sessions: glyph(
    <>
      <rect x="3" y="3.5" width="14" height="13" rx="3" />
      <path d="M6.5 8l2 2-2 2M10.5 12h3" />
    </>,
  ),
  context: glyph(<path d="M5 3h8.5A1.5 1.5 0 0 1 15 4.5V17H6.5A1.5 1.5 0 0 1 5 15.5V3zM5 15.5A1.5 1.5 0 0 1 6.5 14H15" />),
  agents: glyph(
    <>
      <circle cx="5" cy="6" r="2" />
      <circle cx="15" cy="6" r="2" />
      <circle cx="10" cy="15" r="2" />
      <path d="M6.5 7.5l2.6 5.6M13.5 7.5l-2.6 5.6" />
    </>,
  ),
  settings: glyph(
    <>
      <path d="M4 6h7M14.5 6H16M4 14h2M9.5 14H16" />
      <circle cx="12.5" cy="6" r="1.7" />
      <circle cx="7.5" cy="14" r="1.7" />
    </>,
  ),
};

export const INBOX_ICON = glyph(<path d="M3.5 11h3.5l1.5 2h3l1.5-2h3.5M4.5 5h11l1 6v4.5h-13V11z" />);
