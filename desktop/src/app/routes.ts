// The order is the sidebar's order, and Ctrl/⌘ 1-7 follow it (Tasks is ⌘2, issue #86).
export const ROUTES = [
  { id: "home", label: "Home" },
  { id: "tasks", label: "Tasks" },
  { id: "sessions", label: "Sessions" },
  { id: "projects", label: "Projects" },
  { id: "context", label: "Context" },
  { id: "agents", label: "Agents" },
  { id: "settings", label: "Settings" },
] as const;

export type RouteId = (typeof ROUTES)[number]["id"];

/** A page. `task` (a sample task preview) is a sub-page that is not a primary destination. */
export interface Location {
  page: RouteId | "task";
  /** Registration id or sample project name for `projects`, session id for `sessions`, `new` or a saved task id for
   * `tasks`, sample task id for `task`. */
  param?: string;
}

export function parseHash(hash: string): Location {
  const [id, ...rest] = hash.replace(/^#\/?/, "").split("/");
  let param: string | undefined;
  if (rest.length) {
    const raw = rest.join("/");
    try {
      param = decodeURIComponent(raw);
    } catch {
      param = raw;
    }
  }
  if (id === "task") return { page: id, param };
  const route = ROUTES.find((r) => r.id === id)?.id ?? "home";
  return { page: route, param: route === "projects" || route === "sessions" || route === "tasks" ? param : undefined };
}

export function hrefFor(page: Location["page"], param?: string): string {
  return `#/${page}${param ? `/${encodeURIComponent(param)}` : ""}`;
}

/** The rail destination to mark as current; sub-pages belong to the page that opens them. */
export function railRoute(loc: Location): RouteId | undefined {
  if (loc.page === "task") return undefined;
  return loc.page;
}
