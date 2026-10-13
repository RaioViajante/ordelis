import { useEffect, useRef } from "react";
import { INBOX_ICON, RAIL_ICONS, SynapseMark } from "../components/icons";
import { SampleBar } from "../components/SampleBar";
import { Button } from "../components/ui";
import { Agents } from "../pages/Agents";
import { Context } from "../pages/Context";
import { Home } from "../pages/Home";
import { Projects } from "../pages/Projects";
import { Sessions } from "../pages/Sessions";
import { Settings } from "../pages/Settings";
import { DraftPreview, TaskPreview } from "../pages/Preview";
import { useSampleData } from "./SampleData";
import { hrefFor, railRoute, ROUTES } from "./routes";
import { useRoute } from "./useRoute";
import { useWorkspace, WorkspaceProvider } from "./Workspace";

const SAMPLE_ROUTES = ["home", "projects", "sessions", "context", "task", "preview"];
const PAGE_LABELS = { task: "Task preview", preview: "Task preview" };

export function Shell() {
  return (
    <WorkspaceProvider>
      <ShellFrame />
    </WorkspaceProvider>
  );
}

function ShellFrame() {
  const [loc, go] = useRoute();
  const route = loc.page;
  const current = railRoute(loc);
  const { on } = useSampleData();
  const { announcement } = useWorkspace();
  const mainRef = useRef<HTMLElement>(null);
  const first = useRef(true);

  // After a navigation, move focus to the page so keyboard and screen-reader users land on the new content.
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    mainRef.current?.focus();
  }, [route, loc.param]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return;
      const target = ROUTES[Number(e.key) - 1];
      if (!target) return;
      e.preventDefault();
      go(target.id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go]);

  return (
    <div className="app">
      <nav className="rail" aria-label="Main">
        <a className="rail__mark" href={hrefFor("home")} aria-label="Synapse">
          <SynapseMark />
        </a>
        {ROUTES.map((r) => (
          <a
            key={r.id}
            className={`rail__btn${current === r.id ? " is-on" : ""}`}
            href={hrefFor(r.id)}
            aria-label={r.label}
            aria-current={current === r.id ? "page" : undefined}
          >
            {RAIL_ICONS[r.id]}
          </a>
        ))}
        <span className="rail__sp" />
        <Button
          variant="ghost"
          className="rail__btn rail__badge"
          reason="The inbox arrives with task planning. There is nothing waiting on you."
          aria-label="Waiting on you"
        >
          {INBOX_ICON}
          {on && SAMPLE_ROUTES.includes(route) && <b aria-hidden="true">3</b>}
        </Button>
      </nav>

      <main className="page" ref={mainRef} tabIndex={-1} aria-label={route === "task" || route === "preview" ? PAGE_LABELS[route] : ROUTES.find((r) => r.id === route)?.label}>
        <div className="page__scroll">
          {route === "home" && <Home go={go} />}
          {route === "projects" && <Projects param={loc.param} go={go} />}
          {route === "task" && <TaskPreview id={loc.param} go={go} />}
          {route === "preview" && <DraftPreview go={go} />}
          {route === "sessions" && <Sessions />}
          {route === "context" && <Context go={go} />}
          {route === "agents" && <Agents />}
          {route === "settings" && <Settings />}
        </div>
        {on && SAMPLE_ROUTES.includes(route) && <SampleBar />}
      </main>
      <div className="sr" role="status" aria-live="polite">
        <span key={announcement.id}>{announcement.text}</span>
      </div>
    </div>
  );
}
