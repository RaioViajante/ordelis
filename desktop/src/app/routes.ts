export const ROUTES = [
  { id: "home", label: "Home" },
  { id: "projects", label: "Projects" },
  { id: "sessions", label: "Sessions" },
  { id: "context", label: "Context" },
  { id: "agents", label: "Agents" },
  { id: "settings", label: "Settings" },
] as const;

export type RouteId = (typeof ROUTES)[number]["id"];

export function parseHash(hash: string): RouteId {
  const id = hash.replace(/^#\/?/, "");
  return ROUTES.find((r) => r.id === id)?.id ?? "home";
}

export function hrefFor(id: RouteId): string {
  return `#/${id}`;
}
