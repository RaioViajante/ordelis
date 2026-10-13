export const ROUTES = [
  { id: "home", label: "Home" },
  { id: "projects", label: "Projects" },
  { id: "sessions", label: "Sessions" },
  { id: "context", label: "Context" },
  { id: "agents", label: "Agents" },
  { id: "settings", label: "Settings" },
] as const;

export type RouteId = (typeof ROUTES)[number]["id"];

/** A page. `task` and `preview` are sub-pages that are not primary destinations. */
export interface Location {
  page: RouteId | "task" | "preview";
  /** Project name for `projects`, task id for `task`. */
  param?: string;
}

export function parseHash(hash: string): Location {
  const [id, ...rest] = hash.replace(/^#\/?/, "").split("/");
  const param = rest.length ? decodeURIComponent(rest.join("/")) : undefined;
  if (id === "task" || id === "preview") return { page: id, param };
  const route = ROUTES.find((r) => r.id === id)?.id ?? "home";
  return { page: route, param: route === "projects" ? param : undefined };
}

export function hrefFor(page: Location["page"], param?: string): string {
  return `#/${page}${param ? `/${encodeURIComponent(param)}` : ""}`;
}

/** The rail destination to mark as current; sub-pages belong to the page that opens them. */
export function railRoute(loc: Location): RouteId | undefined {
  if (loc.page === "task") return undefined;
  return loc.page === "preview" ? "home" : loc.page;
}
