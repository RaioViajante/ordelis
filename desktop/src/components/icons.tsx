// Sidebar glyphs. Paths are copied from the approved desktop export (Components and Home boards, 24 px grid,
// 1.6 px round stroke), not redrawn. The product symbol is deliberately absent: no approved vector master
// exists yet (see docs/design/principles.md).
import type { ReactNode } from "react";
import type { RouteId } from "../app/routes";

const glyph = (children: ReactNode) => (
  <svg className="ic" viewBox="0 0 24 24" aria-hidden="true">
    {children}
  </svg>
);

export const NAV_ICONS: Record<RouteId | "tasks", ReactNode> = {
  home: glyph(<path d="M4 10.5 12 4l8 6.5V20h-5.5v-5.5h-5V20H4z" />),
  tasks: glyph(<path d="M10 6h10M10 12h10M10 18h10M4 6l1.5 1.5L8 5M4 12l1.5 1.5L8 11M4 18h3" />),
  sessions: glyph(
    <>
      <rect x="4" y="4" width="16" height="6.5" rx="1.8" />
      <rect x="4" y="13.5" width="16" height="6.5" rx="1.8" />
    </>,
  ),
  projects: glyph(<path d="M3.5 7a2 2 0 0 1 2-2H10l2 2.5h6.5a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" />),
  context: glyph(<path d="M12 6.5C10 5 7 4.5 4 5v13c3-.5 6 0 8 1.5 2-1.5 5-2 8-1.5V5c-3-.5-6 0-8 1.5zM12 6.5v13" />),
  agents: glyph(
    <>
      <circle cx="9" cy="8.5" r="3.5" />
      <path d="M3 19.5c.7-3.2 3.1-5 6-5s5.3 1.8 6 5M15.5 5.5a3.5 3.5 0 0 1 0 6.5M17.5 14.8c1.8.7 3 2.3 3.5 4.7" />
    </>,
  ),
  settings: glyph(
    <>
      <path d="M4 7h10M18 7h2M4 17h4M12 17h8" />
      <circle cx="16" cy="7" r="2" />
      <circle cx="10" cy="17" r="2" />
    </>,
  ),
};

export const PLUS_ICON = glyph(<path d="M12 5v14M5 12h14" />);

export const SEARCH_ICON = glyph(
  <>
    <circle cx="11" cy="11" r="6.5" />
    <path d="m20 20-4.2-4.2" />
  </>,
);
