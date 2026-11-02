import { NAV_ICONS, PLUS_ICON, SEARCH_ICON } from "./icons";
import { Button } from "./ui";
import { useSampleData } from "../app/SampleData";
import { hrefFor, ROUTES, type Location, type RouteId } from "../app/routes";
import { useWorkspace } from "../app/Workspace";
import { MOCK_PROJECTS } from "../fixtures/mock";

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

/** `sampleBadge` is true on the pages that show sample data, so the count is never shown outside them. */
export function Sidebar({ loc, current, sampleBadge }: { loc: Location; current: RouteId | undefined; sampleBadge: boolean }) {
  const { on } = useSampleData();
  const { project } = useWorkspace();

  return (
    <nav className="side" aria-label="Main">
      <a className="side__brand" href={hrefFor("home")}>
        ordelis
      </a>
      <Button variant="go" className="side__new" reason="Task planning isn't built yet. Nothing can be started from here.">
        {PLUS_ICON}
        <span className="side__label">New Task</span>
      </Button>
      <Button className="side__search" reason="Search isn't built yet.">
        {SEARCH_ICON}
        <span className="side__label">Search</span>
      </Button>

      <NavLink id="home" current={current} />
      <Button variant="bare" className="nav" reason="Tasks and the inbox arrive with task planning. Nothing is waiting on you.">
        {NAV_ICONS.tasks}
        <span className="nav__label">Tasks</span>
        {sampleBadge && (
          <>
            <span className="badge" aria-hidden="true">
              3
            </span>
            <span className="sr">, 3 sample items need you</span>
          </>
        )}
      </Button>
      {ROUTES.filter((r) => r.id !== "home" && r.id !== "settings").map((r) => (
        <NavLink key={r.id} id={r.id} current={current} />
      ))}

      <div className="side__projects">
        <div className="side__sec">Projects</div>
        {on ? (
          MOCK_PROJECTS.map((p) => {
            const here = loc.page === "projects" && project === p.name;
            return (
              <a key={p.name} className={`nav${here ? " is-on" : ""}`} href={hrefFor("projects", p.name)}>
                <span className="dot" aria-hidden="true" />
                <span className="nav__label">{p.name}</span>
              </a>
            );
          })
        ) : (
          <p className="side__none">No projects yet.</p>
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
