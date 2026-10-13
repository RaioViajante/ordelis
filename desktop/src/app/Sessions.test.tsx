import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { filterSessions, MOCK_SESSIONS, offsetLabel, orderedEvents, STATES, usableSessions } from "../fixtures/sessions";
import { SampleDataProvider } from "./SampleData";
import { parseHash } from "./routes";
import { Shell } from "./Shell";

function renderApp(sample = true) {
  return render(
    <SampleDataProvider initial={sample}>
      <Shell />
    </SampleDataProvider>,
  );
}

const rail = () => within(screen.getByRole("navigation", { name: "Main" }));
const list = () => screen.getByRole("list", { name: "Sample sessions" });
const rows = () => within(list()).getAllByRole("listitem");
const search = () => screen.getByRole("searchbox", { name: /Search sessions/ });

async function openSessions(user: ReturnType<typeof userEvent.setup>) {
  await user.click(rail().getByRole("link", { name: "Sessions" }));
  await screen.findByRole("heading", { level: 1, name: "Sessions" });
}

describe("session fixtures", () => {
  it("covers all five states and labels every record as synthetic", () => {
    expect(new Set(usableSessions().map((s) => s.state))).toEqual(new Set(STATES));
    for (const s of MOCK_SESSIONS) expect(s.provenance).toMatch(/Synthetic .* invented/);
  });

  it("orders events by recorded offset, not array position", () => {
    const out = orderedEvents([
      { t: 30, kind: "output", text: "c" },
      { t: 0, kind: "start", text: "a" },
      { t: 30, kind: "error", text: "d" },
      { t: 10, kind: "command", text: "b" },
    ]);
    expect(out.map((e) => e.text)).toEqual(["a", "b", "c", "d"]);
    expect(offsetLabel(641)).toBe("+10:41");
  });

  it("combines state, project and search with AND", () => {
    const all = usableSessions();
    expect(filterSessions(all, { state: "stopped", project: "all", query: "" }).map((s) => s.id)).toEqual(["b71c"]);
    expect(filterSessions(all, { state: "all", project: "mochi-web", query: "" })).toEqual([]);
    expect(filterSessions(all, { state: "all", project: "all", query: "claude-code pagination" }).map((s) => s.id)).toEqual(["9c1e"]);
    expect(filterSessions(all, { state: "paused", project: "atlas-api", query: "codex" })).toEqual([]);
  });

  it("keeps a malformed encoded session id from throwing", () => {
    expect(parseHash("#/sessions/%E0%A4%A")).toEqual({ page: "sessions", param: "%E0%A4%A" });
    expect(parseHash("#/sessions/b71c")).toEqual({ page: "sessions", param: "b71c" });
  });
});

describe("Sessions list", () => {
  it("renders every sample session with state, project and disclosure", async () => {
    const user = userEvent.setup();
    renderApp();
    await openSessions(user);
    expect(rows()).toHaveLength(5);
    expect(within(list()).getByText(/blocked · authorization/)).toBeInTheDocument();
    expect(screen.getByText("5 of 5 sample sessions")).toBeInTheDocument();
    expect(screen.getByRole("note", { name: "Sample data notice" })).toBeInTheDocument();
  });

  it("filters by state and project, and combines them with search", async () => {
    const user = userEvent.setup();
    renderApp();
    await openSessions(user);
    await user.selectOptions(screen.getByLabelText("State"), "stopped");
    expect(rows()).toHaveLength(1);
    expect(screen.getByText("1 of 5 sample sessions")).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText("State"), "all");
    await user.type(search(), "claude-code");
    expect(rows()).toHaveLength(3);
    await user.selectOptions(screen.getByLabelText("State"), "finished");
    expect(rows()).toHaveLength(1);
    expect(within(list()).getByText(/Fix pagination/)).toBeInTheDocument();
  });

  it("shows a filtered-empty state that can be cleared", async () => {
    const user = userEvent.setup();
    renderApp();
    await openSessions(user);
    await user.selectOptions(screen.getByLabelText("Project"), "mochi-web");
    expect(screen.getByRole("heading", { name: "No sessions match" })).toBeInTheDocument();
    expect(screen.getByText("0 of 5 sample sessions")).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Clear filters" })[0]);
    expect(rows()).toHaveLength(5);
  });

  it("marks one row selected and drops the comparison when filters hide it", async () => {
    const user = userEvent.setup();
    renderApp();
    await openSessions(user);
    const pressed = () => within(list()).getAllByRole("button", { pressed: true });
    expect(pressed()).toHaveLength(1);
    expect(screen.getByRole("heading", { level: 2, name: /Pick up from codex · b71c/ })).toBeInTheDocument();
    await user.click(within(list()).getByRole("button", { name: /Speed up note search/ }));
    expect(pressed()).toHaveLength(1);
    expect(screen.getByRole("heading", { level: 2, name: /Pick up from claude-code · e81a/ })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Session e81a selected");
    await user.selectOptions(screen.getByLabelText("State"), "paused");
    expect(screen.getByRole("heading", { name: "No session selected" })).toBeInTheDocument();
  });

  it("is operable from the keyboard", async () => {
    const user = userEvent.setup();
    renderApp();
    await openSessions(user);
    search().focus();
    await user.keyboard("codex");
    expect(rows()).toHaveLength(1);
    await user.tab(); // State
    await user.tab(); // Project
    await user.tab(); // Clear filters
    await user.tab(); // row button
    await user.keyboard("{Enter}");
    expect(within(list()).getByRole("button", { pressed: true })).toHaveFocus();
    await user.tab(); // Details link
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { level: 1, name: "Move auth to server-side sessions" })).toBeInTheDocument();
  });

  it("shows no synthetic sessions, and offers them back, when sample data is off", async () => {
    const user = userEvent.setup();
    renderApp(false);
    await openSessions(user);
    expect(screen.getByRole("heading", { name: "No sessions yet" })).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Sample sessions" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Show sample data" }));
    expect(rows()).toHaveLength(5);
  });
});

describe("Session detail", () => {
  it("opens from the list and from a direct route", async () => {
    window.location.hash = "#/sessions/9c1e";
    renderApp();
    expect(await screen.findByRole("heading", { level: 1, name: "Fix pagination on /notes" })).toBeInTheDocument();
    expect(screen.getByText(/session 9c1e · synthetic/)).toBeInTheDocument();
    expect(rail().getByRole("link", { name: "Sessions" })).toHaveAttribute("aria-current", "page");
  });

  it("lists the timeline oldest first and labels it as recorded, not live", async () => {
    window.location.hash = "#/sessions/b71c";
    renderApp();
    const tl = await screen.findByRole("list", { name: /Recorded synthetic events for session b71c, oldest first/ });
    const offsets = within(tl).getAllByRole("listitem").map((li) => li.firstElementChild?.textContent ?? "");
    expect(offsets).toEqual([...offsets].sort());
    expect(offsets[0]).toBe("+00:00");
    expect(screen.getByText(/synthetic example · not live, not streaming/)).toBeInTheDocument();
    expect(screen.getByText(/not output from a running terminal/)).toBeInTheDocument();
  });

  it("says so when an event has no detail instead of inventing one", async () => {
    window.location.hash = "#/sessions/c5d0";
    renderApp();
    expect(await screen.findByText("no detail recorded")).toBeInTheDocument();
  });

  it("states result availability and its limits", async () => {
    window.location.hash = "#/sessions/b71c";
    renderApp();
    const res = (await screen.findByRole("heading", { level: 2, name: "Result" })).closest("section")!;
    expect(within(res).getByText("partial")).toBeInTheDocument();
    expect(within(res).getByText(/untested, not passing/)).toBeInTheDocument();
  });

  it("distinguishes measured, estimated, reported and unknown usage", async () => {
    window.location.hash = "#/sessions/9c1e";
    renderApp();
    const table = await screen.findByRole("table", { name: /Usage for sample session 9c1e/ });
    const basisOf = (label: string) => within(within(table).getByRole("row", { name: new RegExp(`^${label}`) })).getAllByRole("cell")[1];
    expect(basisOf("Ran")).toHaveTextContent("measured");
    expect(basisOf("Exit code")).toHaveTextContent("reported");
    expect(basisOf("Cost")).toHaveTextContent("unknown");
    expect(within(within(table).getByRole("row", { name: /^Cost/ })).getByText("unknown", { selector: "td.mono" })).toBeInTheDocument();
    window.location.hash = "#/sessions/b71c";
    expect(await screen.findByRole("table", { name: /Usage for sample session b71c/ })).toBeInTheDocument();
    expect(within(screen.getByRole("table", { name: /Usage for sample session b71c/ })).getByText("estimated")).toBeInTheDocument();
  });

  it("handles unknown and malformed ids safely", async () => {
    const user = userEvent.setup();
    window.location.hash = "#/sessions/nope";
    renderApp();
    expect(await screen.findByRole("heading", { name: "Session not found" })).toBeInTheDocument();
    window.location.hash = "#/sessions/%E0%A4%A";
    expect(await screen.findByText(/no sample session has the id “%E0%A4%A”/)).toBeInTheDocument();
    window.location.hash = "#/sessions/%3Cb%3Ex";
    expect(await screen.findByText(/has the id “<b>x”/)).toBeInTheDocument();
    expect(document.querySelector(".empty b")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Back to sessions" }));
    expect(await screen.findByRole("list", { name: "Sample sessions" })).toBeInTheDocument();
  });

  it("does not expose a synthetic session when sample data is off", async () => {
    window.location.hash = "#/sessions/b71c";
    renderApp(false);
    expect(await screen.findByRole("heading", { name: "Session not found" })).toBeInTheDocument();
    expect(screen.queryByText("Move auth to server-side sessions")).not.toBeInTheDocument();
  });

  it("restores list and detail through browser history", async () => {
    const user = userEvent.setup();
    window.history.replaceState(null, "", "#/sessions");
    renderApp();
    await user.click(await screen.findByRole("link", { name: /Open details for session e81a/ }));
    expect(await screen.findByRole("heading", { level: 1, name: "Speed up note search" })).toBeInTheDocument();
    window.history.back();
    expect(await screen.findByRole("list", { name: "Sample sessions" })).toBeInTheDocument();
    window.history.forward();
    await waitFor(() => expect(screen.getByRole("heading", { level: 1, name: "Speed up note search" })).toBeInTheDocument());
  });

  it("keeps the selected session in sync between list and detail", async () => {
    const user = userEvent.setup();
    window.location.hash = "#/sessions/e81a";
    renderApp();
    await screen.findByRole("heading", { level: 1, name: "Speed up note search" });
    await user.click(screen.getByRole("link", { name: "← All sessions" }));
    const sel = await screen.findByRole("button", { name: /Speed up note search/, pressed: true });
    expect(sel).toBeInTheDocument();
  });
});

describe("continuity", () => {
  it("explains the three options across all six dimensions", async () => {
    window.location.hash = "#/sessions/b71c";
    renderApp();
    const table = await screen.findByRole("table", { name: /How continuing, starting a new run and repeating/ });
    for (const h of ["Continue this session", "New run with this context", "Repeat with new settings"]) {
      expect(within(table).getByRole("columnheader", { name: new RegExp(h) })).toBeInTheDocument();
    }
    for (const r of ["CLI conversation", "Worktree", "Files from this run", "Context passed", "Agent / model", "Authorization"]) {
      expect(within(table).getByRole("rowheader", { name: r })).toBeInTheDocument();
    }
    expect(within(table).getByText("new, from main @ 9b04e7d")).toBeInTheDocument();
    expect(within(table).getByText(/resume is advertised for codex, not verified/)).toBeInTheDocument();
  });

  it("does not claim a worktree or files for a session that never began", async () => {
    window.location.hash = "#/sessions/c5d0";
    renderApp();
    const table = await screen.findByRole("table", { name: /How continuing/ });
    expect(within(table).getByText("none exists")).toBeInTheDocument();
    expect(within(table).getAllByText("none produced")).toHaveLength(2);
    expect(within(table).getByText(/resume support for kimi is unknown/)).toBeInTheDocument();
  });

  it("keeps every execution control disabled, described, inert and out of storage", async () => {
    const user = userEvent.setup();
    window.location.hash = "#/sessions/b71c";
    renderApp();
    await screen.findByRole("table", { name: /How continuing/ });
    const before = window.location.hash;
    for (const name of ["Try to resume", "Plan a new run", "Change settings"]) {
      const b = screen.getByRole("button", { name });
      expect(b).toHaveAttribute("aria-disabled", "true");
      expect(b).toHaveAccessibleDescription(/never|isn't connected/);
      await user.click(b);
    }
    expect(window.location.hash).toBe(before);
    expect(screen.queryByRole("button", { name: /retry|fork|restart|resume session|stop|pause/i })).toBeNull();
  });
});

describe("cross-page navigation", () => {
  it("links Projects worktrees, task previews and sessions to each other", async () => {
    const user = userEvent.setup();
    window.location.hash = "#/projects/atlas-api";
    renderApp();
    await user.click(await screen.findByRole("link", { name: "Sample session b71c" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Move auth to server-side sessions" })).toBeInTheDocument();
    expect(screen.getByText(/codex/, { selector: ".ag" })).toBeInTheDocument();

    await user.click(screen.getByRole("link", { name: /claude-code · a3f2/ }));
    expect(await screen.findByText(/session a3f2 · synthetic/)).toBeInTheDocument();

    await user.click(screen.getByRole("link", { name: "Preview task" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Move auth to server-side sessions" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /codex · b71c · stopped/ })).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: /claude-code · a3f2 · paused/ }));
    await user.click(await screen.findByRole("link", { name: "atlas-api" }));
    expect(await screen.findByRole("heading", { level: 2, name: "atlas-api" })).toBeInTheDocument();
  });

  it("states there are no sample sessions for a task without any", async () => {
    window.location.hash = "#/task/onboarding-copy";
    renderApp();
    expect(await screen.findByText("none in the sample data")).toBeInTheDocument();
  });

  it("keeps the six rail destinations and shortcuts", async () => {
    const user = userEvent.setup();
    window.location.hash = "#/sessions/b71c";
    renderApp();
    expect(rail().getAllByRole("link")).toHaveLength(7);
    await user.keyboard("{Control>}3{/Control}");
    expect(await screen.findByRole("list", { name: "Sample sessions" })).toBeInTheDocument();
  });
});

describe("layout and boundaries", () => {
  const css = () => readFileSync(join(__dirname, "..", "styles", "index.css"), "utf8");

  it("defines narrow-window rules for the new layouts", () => {
    expect(css()).toMatch(/\.sfilter\s*{[^}]*flex-wrap:\s*wrap/);
    expect(css()).toMatch(/\.tl__i\s*{[^}]*overflow-wrap:\s*anywhere/);
    expect(css()).toMatch(/\.tw\s*{[^}]*overflow-x:\s*auto/);
    expect(css()).toMatch(/@media \(max-width: 1100px\)[\s\S]*?\.split/);
    expect(css()).toMatch(/@media \(max-width: 900px\)[\s\S]*?\.tl__i/);
  });

  it("adds no motion, so reduced-motion needs nothing extra", () => {
    const added = css().slice(css().indexOf(".slist"), css().indexOf("/* Context */"));
    expect(added).not.toMatch(/transition|animation/);
  });

  it("session code adds no privileged calls, storage, network or process access", () => {
    const files = ["fixtures/sessions.ts", "pages/Sessions.tsx", "pages/SessionDetail.tsx", "components/Continuity.tsx"];
    for (const f of files) {
      expect(readFileSync(join(__dirname, "..", f), "utf8"), f).not.toMatch(
        /@tauri-apps|__TAURI__|\binvoke\(|fetch\(|XMLHttpRequest|WebSocket|localStorage|sessionStorage|indexedDB|child_process|node:fs|process\.env/,
      );
    }
  });
});
