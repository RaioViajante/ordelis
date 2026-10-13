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
import { useSampleData } from "./SampleData";
import { hrefFor, ROUTES, type RouteId } from "./routes";
import { useRoute } from "./useRoute";

const SAMPLE_ROUTES: RouteId[] = ["home", "projects", "sessions", "context"];

export function Shell() {
  const [route, go] = useRoute();
  const { on } = useSampleData();
  const mainRef = useRef<HTMLElement>(null);
  const first = useRef(true);

  // After a navigation, move focus to the page so keyboard and screen-reader users land on the new content.
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    mainRef.current?.focus();
  }, [route]);

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
            className={`rail__btn${route === r.id ? " is-on" : ""}`}
            href={hrefFor(r.id)}
            aria-label={r.label}
            aria-current={route === r.id ? "page" : undefined}
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
          {on && <b aria-hidden="true">3</b>}
        </Button>
      </nav>

      <main className="page" ref={mainRef} tabIndex={-1} aria-label={ROUTES.find((r) => r.id === route)?.label}>
        {route === "home" && <Home go={go} />}
        {route === "projects" && <Projects />}
        {route === "sessions" && <Sessions />}
        {route === "context" && <Context />}
        {route === "agents" && <Agents />}
        {route === "settings" && <Settings />}
        {on && SAMPLE_ROUTES.includes(route) && <SampleBar />}
      </main>
    </div>
  );
}
