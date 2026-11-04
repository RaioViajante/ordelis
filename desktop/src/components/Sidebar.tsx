import { NAV_ICONS, PLUS_ICON, SEARCH_ICON } from "./icons";
import { Button } from "./ui";
import { useRegistry } from "../app/Registry";
import { hrefFor, ROUTES, type Location, type RouteId } from "../app/routes";

const label = (id: RouteId) => ROUTES.find((r) => r.id === id)!.label;

function NavLink({ id, current }: { id: RouteId; current: RouteId | undefined }) {
  const on = current === id;
  return (
    <a className={`nav${on ? " is-on" : ""}`} href={hrefFor(id)} aria-current={on ? "page" : undefined} data-tip={label(id)}>
      {NAV_ICONS[id]}
      <span className="nav__label">{label(id)}</span>
    </a>
  );
}

export function Sidebar({ loc, current }: { loc: Location; current: RouteId | undefined }) {
  const { desktop, loading, projects } = useRegistry();

  return (
    <nav className="side" aria-label="Main">
      <a className="side__brand" href={hrefFor("home")}>
        ordelis
      </a>
      {/* Writes a saved task; it plans and starts nothing. */}
      <a
        className={`btn btn--go side__new${loc.page === "tasks" && loc.param === "new" ? " is-on" : ""}`}
        href={hrefFor("tasks", "new")}
        aria-current={loc.page === "tasks" && loc.param === "new" ? "page" : undefined}
      >
        {PLUS_ICON}
        <span className="side__label">New Task</span>
      </a>
      <Button className="side__search" reason="Search isn't built yet.">
        {SEARCH_ICON}
        <span className="side__label">Search</span>
      </Button>

      {ROUTES.filter((r) => r.id !== "settings").map((r) => (
        <NavLink key={r.id} id={r.id} current={current} />
      ))}

      <div className="side__projects">
        <div className="side__sec">Projects</div>
        {projects.map((p) => {
          // The registry id is the identity; names can repeat.
          const here = loc.page === "projects" && loc.param === p.id;
          return (
            <a key={p.id} className={`nav${here ? " is-on" : ""}`} href={hrefFor("projects", p.id)} aria-current={here ? "page" : undefined}>
              <span className={`dot${p.status === "available" ? "" : " dot--warn"}`} aria-hidden="true" />
              <span className="nav__label bidi" dir="auto">
                {p.name}
              </span>
              {p.status !== "available" && <span className="sr">, needs attention</span>}
            </a>
          );
        })}
        {projects.length === 0 && (
          <p className="side__none">{loading ? "Loading projects…" : desktop ? "No projects yet." : "Folders can be added in the desktop app."}</p>
        )}
      </div>

      <span className="side__sp" />
      <NavLink id="settings" current={current} />
      <div className="side__ws">
        <span className="side__av" aria-hidden="true">
          PW
        </span>
        <span className="side__ws-t">
          <span>Personal Workspace</span>
          <span className="meta">Stored on this Mac</span>
        </span>
      </div>
    </nav>
  );
}
