import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StrictMode } from "react";
import { addProject, ERROR_CODES, listProjects, readList, removeProject } from "./projects";
import { SampleDataProvider } from "./SampleData";
import { Shell } from "./Shell";

// Mocks exist only here: production code has no fallback answer of any kind. This one stands in for Rust: it keeps a
// registry in memory (which survives a re-render, like the file survives a relaunch) and holds the native folder
// panel open until the test answers it.
interface Rec {
  id: string;
  name: string;
  path_display: string;
  status: "available" | "missing" | "inaccessible" | "changed";
  registered_at_ms: number;
}
const failure = (code: string) => ({ code, message: "Fixed native message that must not be shown." });

let registry: Rec[] = [];
let nextId = 1;
let calls: { cmd: string; args: unknown }[] = [];
let panel: { resolve: (v: unknown) => void; reject: (e: unknown) => void } | null = null;
let listFailure: string | null = null;
let rawList: unknown = null;
let holdList: ((v: unknown) => void)[] | null = null;

const flag = globalThis as { isTauri?: boolean };

function folder(name: string, status: Rec["status"] = "available"): Rec {
  const id = `p${nextId++}`;
  return { id, name, path_display: `~/Code/${name}`, status, registered_at_ms: 1_776_000_000_000 + nextId };
}

function native() {
  flag.isTauri = true;
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (cmd === "list_projects") {
      if (listFailure) return Promise.reject(failure(listFailure));
      const answer = rawList ?? { schema_version: 1, projects: registry.map((p) => ({ ...p })) };
      return holdList ? new Promise((resolve) => holdList!.push(() => resolve(answer))) : answer;
    }
    // The task store is empty here; the task flows are covered in Tasks.test.tsx.
    if (cmd === "list_tasks") return { schema_version: 1, tasks: [] };
    if (cmd === "add_project") {
      if (panel) return Promise.reject(failure("selection_in_progress"));
      return new Promise((resolve, reject) => {
        panel = { resolve, reject };
      });
    }
    if (cmd === "remove_project") {
      const id = (args as { id: string }).id;
      const found = registry.some((p) => p.id === id);
      registry = registry.filter((p) => p.id !== id);
      return { outcome: found ? "removed" : "not_found" };
    }
    return Promise.reject(failure("not_allowed"));
  });
}

/** The user picks a folder in the native panel. A folder already registered answers already_registered. */
async function choose(name: string, status: Rec["status"] = "available") {
  const open = panel;
  if (!open) throw new Error("no panel is open");
  panel = null;
  const existing = registry.find((p) => p.name === name);
  const project = existing ?? folder(name, status);
  if (!existing) registry.push(project);
  await act(async () => open.resolve({ outcome: existing ? "already_registered" : "added", project: { ...project } }));
}
async function cancel() {
  const open = panel;
  panel = null;
  await act(async () => open?.resolve({ outcome: "cancelled" }));
}
async function refuse(code: string) {
  const open = panel;
  panel = null;
  await act(async () => open?.reject(failure(code)));
}
const count = (cmd: string) => calls.filter((c) => c.cmd === cmd).length;

beforeEach(() => {
  registry = [];
  nextId = 1;
  calls = [];
  panel = null;
  listFailure = null;
  rawList = null;
  holdList = null;
});
afterEach(async () => {
  // Settle an open panel so the adapter's shared request never leaks into the next test.
  if (panel) await cancel();
  delete flag.isTauri;
  clearMocks();
});

describe("project adapter", () => {
  it("makes no native call and answers preview outside the desktop app", async () => {
    mockIPC(() => {
      throw new Error("must not be called");
    });
    expect(await listProjects()).toEqual({ kind: "preview" });
    expect(await addProject()).toEqual({ kind: "preview" });
    expect(await removeProject("p1")).toEqual({ kind: "preview" });
    expect(calls).toEqual([]);
  });

  it("lists, adds and removes, sending no path and only the registration id", async () => {
    native();
    registry.push(folder("alpha"));
    const listed = await listProjects();
    expect(listed.kind === "ok" && listed.projects.map((p) => p.name)).toEqual(["alpha"]);

    const adding = addProject();
    await choose("beta");
    expect(await adding).toMatchObject({ kind: "added", project: { id: "p2", name: "beta" } });

    expect(await removeProject("p1")).toEqual({ kind: "removed" });
    expect(await removeProject("p1")).toEqual({ kind: "not_found" });

    // list_projects and add_project take nothing; remove_project takes the id and no other key.
    expect(calls.filter((c) => c.cmd !== "remove_project").every((c) => c.args === undefined || Object.keys(c.args as object).length === 0)).toBe(true);
    expect(calls.filter((c) => c.cmd === "remove_project").map((c) => c.args)).toEqual([{ id: "p1" }, { id: "p1" }]);
  });

  it("reports a cancelled selection as cancelled, not as an error", async () => {
    native();
    const adding = addProject();
    await cancel();
    expect(await adding).toEqual({ kind: "cancelled" });
  });

  it("reports an already registered folder with its record and live status", async () => {
    native();
    registry.push(folder("alpha", "changed"));
    const adding = addProject();
    await choose("alpha");
    expect(await adding).toMatchObject({ kind: "already_registered", project: { id: "p1", status: "changed" } });
  });

  it("keeps only a known error code and drops the native message", async () => {
    native();
    for (const code of ERROR_CODES) {
      const adding = addProject();
      await refuse(code);
      expect(await adding).toEqual({ kind: "error", code });
    }
    const adding = addProject();
    await refuse("something_new");
    expect(await adding).toEqual({ kind: "unreadable" });
  });

  it("shares one native panel between concurrent adds and opens a new one after it closes", async () => {
    native();
    const first = addProject();
    const second = addProject();
    expect(count("add_project")).toBe(1);
    await choose("alpha");
    const [a, b] = await Promise.all([first, second]);
    expect(a).toEqual(b);
    const third = addProject();
    expect(count("add_project")).toBe(2);
    await cancel();
    await third;
  });

  it("answers selection_in_progress when Rust already has a panel open (a reloaded page)", async () => {
    native();
    // A panel left open by a page that no longer exists: Rust refuses a second one.
    panel = { resolve: () => {}, reject: () => {} };
    expect(await addProject()).toEqual({ kind: "error", code: "selection_in_progress" });
    expect(count("add_project")).toBe(1);
    panel = null;
  });

  it("never sends an id that is not a registration id", async () => {
    native();
    expect(await removeProject("../etc")).toEqual({ kind: "error", code: "invalid_input" });
    expect(await removeProject("")).toEqual({ kind: "error", code: "invalid_input" });
    expect(count("remove_project")).toBe(0);
  });

  it("refuses a list outside the contract instead of showing half of it", () => {
    const good = { id: "p1", name: "a", path_display: "~/a", status: "available", registered_at_ms: 1 };
    expect(readList({ schema_version: 1, projects: [good] }).kind).toBe("ok");
    const bad: unknown[] = [
      { schema_version: 2, projects: [good] },
      { schema_version: 1, projects: [good], extra: 1 },
      { schema_version: 1, projects: [{ ...good, id: "x1" }] },
      { schema_version: 1, projects: [{ ...good, status: "gone" }] },
      { schema_version: 1, projects: [{ ...good, device: 4 }] },
      { schema_version: 1, projects: [{ ...good, registered_at_ms: -1 }] },
      { schema_version: 1, projects: [good, good] },
      { schema_version: 1, projects: "none" },
      null,
    ];
    for (const raw of bad) expect(readList(raw), JSON.stringify(raw)).toEqual({ kind: "unreadable" });
  });
});

function renderApp(route = "projects", { strict = false, sample = false } = {}) {
  window.location.hash = `#/${route}`;
  const tree = (
    <SampleDataProvider initial={sample}>
      <Shell />
    </SampleDataProvider>
  );
  return render(strict ? <StrictMode>{tree}</StrictMode> : tree);
}
const sidebar = () => within(screen.getByRole("navigation", { name: "Main" }));
const main = () => within(screen.getByRole("main"));
const status = () => screen.getByRole("status", { hidden: true });
const addButton = () => main().getByRole("button", { name: /^(Add a folder…|Choose a folder…)$/ });
// The button waits for the first list, so a click before that does nothing.
async function openPanel(user: ReturnType<typeof userEvent.setup>) {
  await waitFor(() => expect(main().getByRole("button", { name: "Add a folder…" })).not.toHaveAttribute("aria-disabled"));
  await user.click(main().getByRole("button", { name: "Add a folder…" }));
  await waitFor(() => expect(panel).not.toBeNull());
}

describe("Projects page", () => {
  it("shows a loading state, then the empty state with an enabled Add button", async () => {
    native();
    holdList = [];
    renderApp();
    expect(await main().findByText("Loading your folders…", { selector: "p" })).toBeInTheDocument();
    expect(main().getByRole("button", { name: "Add a folder…" })).toHaveAccessibleDescription("Loading your projects…");
    expect(sidebar().getByText("Loading projects…")).toBeInTheDocument();
    const release = holdList.shift();
    holdList = null;
    await act(async () => release?.(null));
    expect(await main().findByRole("heading", { name: "No projects yet" })).toBeInTheDocument();
    expect(addButton()).not.toHaveAttribute("aria-disabled");
    expect(main().getByText(/does not read its files, run agents, use Git or open a terminal/)).toBeInTheDocument();
    expect(sidebar().getByText("No projects yet.")).toBeInTheDocument();
    expect(count("list_projects")).toBe(1);
  });

  it("lists registered folders with every status in words, and explains each problem", async () => {
    native();
    registry.push(folder("alpha"), folder("beta", "missing"), folder("gamma", "inaccessible"), folder("delta", "changed"));
    const user = userEvent.setup();
    renderApp();
    const group = await main().findByRole("group", { name: "Your folders" });
    expect(within(group).getAllByRole("button")).toHaveLength(4);
    for (const text of ["Available", "Folder not found", "Can't access folder", "Different folder at this path"]) {
      expect(within(group).getByText(text)).toBeInTheDocument();
    }
    // The first folder is selected by default and is a link target by id.
    expect(main().getByRole("heading", { level: 2, name: "alpha" })).toBeInTheDocument();
    await user.click(within(group).getByRole("button", { name: /delta/ }));
    expect(window.location.hash).toBe("#/projects/p4");
    expect(await main().findByRole("heading", { level: 2, name: "delta" })).toBeInTheDocument();
    expect(main().getByText(/A different folder is now at the saved location/)).toBeInTheDocument();
    await user.click(within(group).getByRole("button", { name: /beta/ }));
    expect(await main().findByText(/Nothing was found at the saved location/)).toBeInTheDocument();
    // Registered only: what Ordelis has not done.
    expect(main().getByText(/Registered only\. Ordelis has not read this folder/)).toBeInTheDocument();
  });

  it("adds a folder through the native panel and selects it, refreshing every view", async () => {
    native();
    const user = userEvent.setup();
    renderApp();
    await openPanel(user);
    // While the panel is open the button says so, stays focusable and cannot open a second panel.
    const busy = addButton();
    expect(busy).toHaveTextContent("Choose a folder…");
    expect(busy).toHaveAttribute("aria-disabled", "true");
    await user.click(busy);
    expect(count("add_project")).toBe(1);

    await choose("my-site");
    expect(await main().findByRole("heading", { level: 2, name: "my-site" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/projects/p1");
    expect(main().getByText("Added my-site. Ordelis saved only its location and has not read its contents.")).toBeInTheDocument();
    expect(status()).toHaveTextContent("Added my-site");
    // Sidebar, with the registry id as identity.
    const link = sidebar().getByRole("link", { name: "my-site" });
    expect(link).toHaveAttribute("href", "#/projects/p1");
    expect(link).toHaveAttribute("aria-current", "page");
    expect(count("list_projects")).toBe(2); // loaded on start, read again after the change
  });

  it("treats a cancelled selection as nothing: no message, no refresh, focus stays on the button", async () => {
    native();
    const user = userEvent.setup();
    renderApp();
    await openPanel(user);
    await cancel();
    await waitFor(() => expect(addButton()).toHaveTextContent("Add a folder…"));
    expect(main().queryByRole("alert")).not.toBeInTheDocument();
    expect(main().queryByText(/Added /)).not.toBeInTheDocument();
    expect(count("list_projects")).toBe(1);
    expect(addButton()).toHaveFocus();
    expect(registry).toEqual([]);
  });

  it("says when a folder is already registered and selects the existing entry", async () => {
    native();
    registry.push(folder("alpha", "changed"), folder("beta"));
    const user = userEvent.setup();
    renderApp();
    await openPanel(user);
    await choose("alpha");
    expect(await main().findByText(/alpha is already in your projects\. A different folder is now at that location\./)).toBeInTheDocument();
    expect(window.location.hash).toBe("#/projects/p1");
    expect(registry).toHaveLength(2);
  });

  it("shows selection_in_progress as a warning, not a failure, and reads the list again", async () => {
    native();
    const user = userEvent.setup();
    renderApp();
    await openPanel(user);
    await refuse("selection_in_progress");
    expect(await main().findByText("A folder selection is already open. Finish or cancel it, then try again.")).toBeInTheDocument();
    expect(main().queryByRole("alert")).not.toBeInTheDocument();
    expect(count("list_projects")).toBe(2);
  });

  it.each([
    ["folder_refused", "That folder is too broad to register as a project. Choose the folder of one project."],
    ["folder_inaccessible", "That folder could not be accessed. Nothing was saved."],
    ["registry_full", "The project list is full. Remove a project to add another."],
    ["registry_newer_version", "The project list was saved by a newer version of Ordelis. Update Ordelis to use it."],
  ])("words %s itself and never shows the native message", async (code, text) => {
    native();
    const user = userEvent.setup();
    renderApp();
    await openPanel(user);
    await refuse(code);
    expect(await main().findByRole("alert")).toHaveTextContent(text);
    expect(screen.queryByText(/Fixed native message/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Synapse/)).not.toBeInTheDocument();
  });

  it("blocks add and remove while the list cannot be read, offers Try again, and has no reset", async () => {
    native();
    registry.push(folder("alpha"));
    listFailure = "registry_corrupt";
    const user = userEvent.setup();
    renderApp();
    const alert = await main().findByRole("alert");
    expect(alert).toHaveTextContent("The project list could not be read. Its file was kept unchanged and nothing was deleted.");
    expect(addButton()).toHaveAttribute("aria-disabled", "true");
    expect(addButton()).toHaveAccessibleDescription("The project list needs attention first.");
    expect(screen.queryByRole("button", { name: /reset/i })).not.toBeInTheDocument();
    listFailure = null;
    await user.click(within(alert).getByRole("button", { name: "Try again" }));
    expect(await main().findByRole("heading", { level: 2, name: "alpha" })).toBeInTheDocument();
    expect(main().queryByRole("alert")).not.toBeInTheDocument();
    expect(addButton()).not.toHaveAttribute("aria-disabled");
    expect(calls.some((c) => c.cmd === "reset_projects")).toBe(false);
  });

  it("refuses a list outside the contract", async () => {
    native();
    rawList = { schema_version: 1, projects: [{ id: "p1", name: "x" }] };
    renderApp();
    expect(await main().findByRole("alert")).toHaveTextContent("Ordelis could not read the answer for the project list.");
  });

  it("removes a registration only after confirmation that names what stays", async () => {
    native();
    registry.push(folder("alpha"), folder("beta"));
    const user = userEvent.setup();
    renderApp("projects/p2");
    const remove = await main().findByRole("button", { name: "Remove from Ordelis…" });
    await user.click(remove);
    const dialog = await screen.findByRole("alertdialog", { name: "Remove beta from Ordelis?" });
    expect(dialog).toHaveAccessibleDescription(/The folder and its files stay exactly where they are/);
    expect(count("remove_project")).toBe(0);
    // The safe choice has focus and is what Escape does.
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(remove).toHaveFocus();
    expect(count("remove_project")).toBe(0);

    await user.click(remove);
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(calls.filter((c) => c.cmd === "remove_project").map((c) => c.args)).toEqual([{ id: "p2" }]);
    expect(main().getByText("Removed beta from Ordelis. The folder and its files were not touched.")).toBeInTheDocument();
    expect(status()).toHaveTextContent("Removed beta");
    expect(sidebar().queryByRole("link", { name: "beta" })).not.toBeInTheDocument();
    expect(sidebar().getByRole("link", { name: "alpha" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/projects");
    await waitFor(() => expect(main().getByRole("group", { name: "Your folders" })).toHaveFocus());
    expect(registry.map((p) => p.name)).toEqual(["alpha"]);
  });

  it("keeps focus inside the confirmation, and a click outside it cancels", async () => {
    native();
    registry.push(folder("alpha"));
    const user = userEvent.setup();
    renderApp();
    await user.click(await main().findByRole("button", { name: "Remove from Ordelis…" }));
    const dialog = await screen.findByRole("alertdialog");
    await user.tab();
    expect(within(dialog).getByRole("button", { name: "Remove" })).toHaveFocus();
    await user.tab();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.tab({ shift: true });
    expect(within(dialog).getByRole("button", { name: "Remove" })).toHaveFocus();
    await user.click(dialog.parentElement as HTMLElement);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(count("remove_project")).toBe(0);
  });

  it("shows an error and keeps the entry when a removal is refused", async () => {
    native();
    registry.push(folder("alpha"));
    const user = userEvent.setup();
    renderApp();
    await user.click(await main().findByRole("button", { name: "Remove from Ordelis…" }));
    registry = []; // someone else removed it: Rust answers not_found
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Remove" }));
    expect(await main().findByText("alpha was already removed.")).toBeInTheDocument();
  });

  it("never offers or calls the registry reset", async () => {
    native();
    listFailure = "registry_corrupt";
    renderApp();
    await main().findByRole("alert");
    expect(screen.queryByText(/reset/i)).not.toBeInTheDocument();
    expect(calls.some((c) => c.cmd === "reset_projects")).toBe(false);
  });

  it("renders names and paths as text, in an isolated direction", async () => {
    native();
    registry.push({ ...folder("<img src=x onerror=alert(1)>"), path_display: "~/עברית/<b>x</b>" });
    renderApp();
    const heading = await main().findByRole("heading", { level: 2, name: /onerror/ });
    expect(heading).toHaveTextContent("<img src=x onerror=alert(1)>");
    expect(heading).toHaveAttribute("dir", "auto");
    expect(document.querySelector("main img, main b")).toBeNull();
  });

  it("loads once under React Strict Mode", async () => {
    native();
    registry.push(folder("alpha"));
    renderApp("projects", { strict: true });
    await main().findByRole("heading", { level: 2, name: "alpha" });
    expect(count("list_projects")).toBe(1);
  });
});

describe("Home, sidebar and persistence", () => {
  it("shows the same registered folders on Home, newest first, and a working first-use Add", async () => {
    native();
    const user = userEvent.setup();
    renderApp("home");
    // No sample data, no folders: first use with a working Add.
    expect(await main().findByRole("heading", { name: "Set up Ordelis" })).toBeInTheDocument();
    await waitFor(() => expect(main().getByRole("button", { name: "Add a folder" })).not.toHaveAttribute("aria-disabled"));
    await user.click(main().getByRole("button", { name: "Add a folder" }));
    await waitFor(() => expect(panel).not.toBeNull());
    await choose("alpha");
    await openPanel(user);
    await choose("beta");
    const recent = await main().findByRole("region", { name: "Recent projects" });
    const names = within(recent).getAllByRole("link").map((l) => l.textContent);
    expect(names[0]).toContain("beta");
    expect(names[1]).toContain("alpha");
    expect(within(recent).getByRole("link", { name: /beta/ })).toHaveAttribute("href", "#/projects/p2");
    expect(sidebar().getAllByRole("link").map((l) => l.getAttribute("href"))).toEqual(expect.arrayContaining(["#/projects/p1", "#/projects/p2"]));
    expect(main().getByText("2 added")).toBeInTheDocument();
  });

  it("keeps sidebar, Home and Projects in step through add and remove", async () => {
    native();
    registry.push(folder("alpha"));
    const user = userEvent.setup();
    renderApp("projects");
    await sidebar().findByRole("link", { name: "alpha" });
    await openPanel(user);
    await choose("beta");
    await sidebar().findByRole("link", { name: "beta" });
    await user.click(sidebar().getByRole("link", { name: "Home" }));
    const recent = await main().findByRole("region", { name: "Recent projects" });
    expect(within(recent).getAllByRole("link")).toHaveLength(2);
    await user.click(within(recent).getByRole("link", { name: /alpha/ }));
    await user.click(await main().findByRole("button", { name: "Remove from Ordelis…" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(sidebar().queryByRole("link", { name: "alpha" })).not.toBeInTheDocument());
    await user.click(sidebar().getByRole("link", { name: "Home" }));
    const after = await main().findByRole("region", { name: "Recent projects" });
    expect(within(after).getAllByRole("link")).toHaveLength(1);
  });

  it("recovers the registered folders after a complete restart", async () => {
    native();
    const user = userEvent.setup();
    const first = renderApp("projects");
    await openPanel(user);
    await choose("my-site");
    await sidebar().findByRole("link", { name: "my-site" });
    // Quit: the whole UI goes away. The registry (Rust's file) is the only thing left.
    first.unmount();
    expect(registry.map((p) => p.name)).toEqual(["my-site"]);
    // Relaunch.
    renderApp("home");
    expect(await sidebar().findByRole("link", { name: "my-site" })).toHaveAttribute("href", "#/projects/p1");
    expect(within(await main().findByRole("region", { name: "Recent projects" })).getByText("my-site")).toBeInTheDocument();
  });

  it("marks a folder that needs attention in the sidebar and on Home", async () => {
    native();
    registry.push(folder("alpha", "missing"));
    renderApp("home");
    expect(await sidebar().findByRole("link", { name: "alpha, needs attention" })).toBeInTheDocument();
    expect(within(await main().findByRole("region", { name: "Recent projects" })).getByText("Folder not found")).toBeInTheDocument();
  });

  it("keeps sample data separate, labelled, and out of the registry views", async () => {
    native();
    registry.push(folder("real-one"));
    const user = userEvent.setup();
    renderApp("projects", { sample: true });
    const sample = await main().findByRole("group", { name: "Sample projects" });
    expect(within(sample).getByText("sample data", { exact: false })).toBeInTheDocument();
    expect(within(sample).getByRole("button", { name: /atlas-api/ })).toBeInTheDocument();
    expect(sidebar().queryByRole("link", { name: "atlas-api" })).not.toBeInTheDocument();
    await user.click(within(sample).getByRole("button", { name: /atlas-api/ }));
    expect(await main().findByRole("heading", { level: 2, name: "atlas-api" })).toBeInTheDocument();
    // A sample project cannot hold a saved task.
    expect(main().getByRole("button", { name: "New task in this project" })).toHaveAttribute("aria-disabled", "true");
    expect(sidebar().getByRole("link", { name: "real-one" })).not.toHaveAttribute("aria-current");
    // Home: sample tasks stay labelled as sample, next to the real folders.
    await user.click(sidebar().getByRole("link", { name: "Home" }));
    expect(await main().findByRole("region", { name: "Recent projects" })).toBeInTheDocument();
    expect(main().getByText("across projects · sample")).toBeInTheDocument();
    expect(screen.getByRole("note", { name: "Sample data notice" })).toBeInTheDocument();
  });

  it("is a clean preview outside the desktop app: no native call, Add disabled with its reason", async () => {
    mockIPC(() => {
      throw new Error("must not be called");
    });
    renderApp("projects");
    const add = await main().findByRole("button", { name: "Add a folder…" });
    expect(add).toHaveAttribute("aria-disabled", "true");
    expect(add).toHaveAccessibleDescription("Adding folders needs the desktop app.");
    expect(sidebar().getByText("Folders can be added in the desktop app.")).toBeInTheDocument();
    expect(main().queryByRole("region", { name: "Recent projects" })).not.toBeInTheDocument();
    expect(calls).toEqual([]);
  });
});
