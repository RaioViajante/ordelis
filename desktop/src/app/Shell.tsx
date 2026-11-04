import { useEffect, useRef } from "react";
import { SampleBar } from "../components/SampleBar";
import { Sidebar } from "../components/Sidebar";
import { Agents } from "../pages/Agents";
import { Context } from "../pages/Context";
import { Home } from "../pages/Home";
import { Projects } from "../pages/Projects";
import { Sessions } from "../pages/Sessions";
import { SessionDetail } from "../pages/SessionDetail";
import { Settings } from "../pages/Settings";
import { TaskPreview } from "../pages/Preview";
import { TaskDetail } from "../pages/TaskDetail";
import { TaskNew } from "../pages/TaskNew";
import { Tasks } from "../pages/Tasks";
import { RegistryProvider } from "./Registry";
import { TasksProvider } from "./TaskState";
import { useSampleData } from "./SampleData";
import { railRoute, ROUTES } from "./routes";
import { useRoute } from "./useRoute";
import { useWorkspace, WorkspaceProvider } from "./Workspace";

const SAMPLE_ROUTES = ["home", "projects", "sessions", "context", "task"];

function pageLabel(route: string, param: string | undefined): string | undefined {
  if (route === "task") return "Task preview";
  if (route === "tasks" && param) return param === "new" ? "New task" : "Task";
  return ROUTES.find((r) => r.id === route)?.label;
}

export function Shell() {
  return (
    <WorkspaceProvider>
      <RegistryProvider>
        <TasksProvider>
          <ShellFrame />
        </TasksProvider>
      </RegistryProvider>
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
      <Sidebar loc={loc} current={current} />

      <main className="page" ref={mainRef} tabIndex={-1} aria-label={pageLabel(route, loc.param)}>
        <div className="page__scroll">
          {route === "home" && <Home go={go} />}
          {route === "tasks" && (loc.param === "new" ? <TaskNew go={go} /> : loc.param ? <TaskDetail key={loc.param} id={loc.param} /> : <Tasks />)}
          {route === "projects" && <Projects param={loc.param} go={go} />}
          {route === "task" && <TaskPreview id={loc.param} go={go} />}
          {route === "sessions" && (loc.param ? <SessionDetail id={loc.param} go={go} /> : <Sessions />)}
          {route === "context" && <Context />}
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
