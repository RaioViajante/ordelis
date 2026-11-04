import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StrictMode } from "react";
import { RegistryProvider } from "./Registry";
import { SampleDataProvider } from "./SampleData";
import { Shell } from "./Shell";
import { TasksProvider, useTasks } from "./TaskState";
import { WorkspaceProvider } from "./Workspace";
import { belongsTo, createTask, getTask, listTasks, readList, readUpdate, textProblem, updateTask, type TaskSummary } from "./tasks";

// Mocks exist only here: production code has no fallback answer of any kind. This one stands in for Rust: it keeps
// tasks and project registrations in memory (which survive a re-render, like the files survive a relaunch), applies
// the D3.1b contract (link states, revisions, conflicts) and records every call.
interface TaskRec {
  id: string;
  title: string;
  description: string;
  status: "draft" | "archived";
  project: { id: string; registered_at_ms: number };
  created_at_ms: number;
  updated_at_ms: number;
  revision: number;
}
interface ProjectRec {
  id: string;
  name: string;
  /** 1 unless a test gives the registration another time (as after a reset that reused the id). */
  registered_at_ms?: number;
}
const failure = (code: string) => ({ code, message: "Fixed native message that must not be shown." });

let tasks: TaskRec[] = [];
let projects: ProjectRec[] = [];
let nextTask = 1;
let calls: { cmd: string; args: unknown }[] = [];
let taskFailure: string | null = null;
/** Per project id, an unreadable registry ("unknown") instead of the link the registrations imply. */
let links: Record<string, "unknown"> = {};
let holdCreate: ((v: unknown) => void)[] | null = null;
let clock = 1_790_000_000_000;

const flag = globalThis as { isTauri?: boolean };

const at = (p: ProjectRec) => p.registered_at_ms ?? 1;
// As Rust computes it: the same id and registration time is linked, the same id with another time is replaced.
function link(project: TaskRec["project"]) {
  if (links[project.id]) return links[project.id];
  const registered = projects.find((p) => p.id === project.id);
  return !registered ? "removed" : at(registered) === project.registered_at_ms ? "linked" : "replaced";
}
const full = (t: TaskRec) => ({ ...t, project: { ...t.project, link: link(t.project) } });
const summary = (t: TaskRec) => {
  const rest: Partial<ReturnType<typeof full>> = full(t);
  delete rest.description; // a list never carries descriptions
  return rest;
};

function native() {
  flag.isTauri = true;
  mockIPC((cmd, raw) => {
    const args = raw as Record<string, unknown>;
    calls.push({ cmd, args });
    if (cmd === "list_projects") {
      return {
        schema_version: 1,
        projects: projects.map((p) => ({ id: p.id, name: p.name, path_display: `~/Code/${p.name}`, status: "available", registered_at_ms: at(p) })),
      };
    }
    if (cmd === "remove_project") {
      const found = projects.some((p) => p.id === args.id);
      projects = projects.filter((p) => p.id !== args.id);
      return { outcome: found ? "removed" : "not_found" };
    }
    if (taskFailure && cmd.endsWith("_task") || (taskFailure && cmd === "list_tasks")) return Promise.reject(failure(taskFailure));
    if (cmd === "list_tasks") return { schema_version: 1, tasks: tasks.map(summary) };
    if (cmd === "get_task") {
      const found = tasks.find((t) => t.id === args.id);
      return found ? { outcome: "found", task: full(found) } : { outcome: "not_found" };
    }
    if (cmd === "create_task") {
      const registered = projects.find((p) => p.id === args.project_id);
      if (!registered) return Promise.reject(failure("project_not_found"));
      clock += 1000;
      const task: TaskRec = {
        id: `t${nextTask++}`,
        title: args.title as string,
        description: args.description as string,
        status: "draft",
        project: { id: registered.id, registered_at_ms: at(registered) },
        created_at_ms: clock,
        updated_at_ms: clock,
        revision: 1,
      };
      tasks.push(task);
      const answer = { outcome: "created", task: full(task) };
      return holdCreate ? new Promise((resolve) => holdCreate!.push(() => resolve(answer))) : answer;
    }
    if (cmd === "update_task") {
      const task = tasks.find((t) => t.id === args.id);
      if (!task) return { outcome: "not_found" };
      if (task.revision !== args.revision) return { outcome: "conflict", task: full(task) };
      clock += 1000;
      Object.assign(task, { title: args.title, description: args.description, status: args.status, updated_at_ms: clock, revision: task.revision + 1 });
      return { outcome: "updated", task: full(task) };
    }
    return Promise.reject(failure("not_allowed"));
  });
}

function seed(title: string, extra: Partial<TaskRec> = {}): TaskRec {
  clock += 1000;
  const task: TaskRec = {
    id: `t${nextTask++}`,
    title,
    description: "",
    status: "draft",
    project: { id: projects[0]?.id ?? "p1", registered_at_ms: projects[0] ? at(projects[0]) : 1 },
    created_at_ms: clock,
    updated_at_ms: clock,
    revision: 1,
    ...extra,
  };
  tasks.push(task);
  return task;
}

/** Another window (or a later version of the same window) changes a saved task. */
function changedElsewhere(id: string, changes: Partial<TaskRec>) {
  const task = tasks.find((t) => t.id === id)!;
  Object.assign(task, changes, { revision: task.revision + 1, updated_at_ms: (clock += 1000) });
}

const count = (cmd: string) => calls.filter((c) => c.cmd === cmd).length;
const lastArgs = (cmd: string) => calls.filter((c) => c.cmd === cmd).at(-1)?.args;
const main = () => within(screen.getByRole("main"));
const rail = () => within(screen.getByRole("navigation", { name: "Main" }));

function renderApp(hash = "#/tasks", sample = true) {
  window.location.hash = hash;
  return render(
    <SampleDataProvider initial={sample}>
      <Shell />
    </SampleDataProvider>,
  );
}

beforeEach(() => {
  tasks = [];
  projects = [{ id: "p1", name: "atlas-api" }];
  nextTask = 1;
  calls = [];
  taskFailure = null;
  links = {};
  holdCreate = null;
});
afterEach(() => {
  delete flag.isTauri;
  clearMocks();
  window.location.hash = "";
});

describe("task adapter", () => {
  it("makes no native call and answers preview outside the desktop app", async () => {
    mockIPC(() => {
      throw new Error("must not be called");
    });
    expect(await listTasks()).toEqual({ kind: "preview" });
    expect(await getTask("t1")).toEqual({ kind: "preview" });
    expect(await createTask({ projectId: "p1", title: "Title", description: "" })).toEqual({ kind: "preview" });
    expect(await updateTask({ id: "t1", revision: 1, title: "Title", description: "", status: "draft" })).toEqual({ kind: "preview" });
    expect(calls).toEqual([]);
  });

  it("sends exactly the reviewed arguments, in snake_case, and never a path", async () => {
    native();
    const created = await createTask({ projectId: "p1", title: "Add rate limiting", description: "Per IP.\nKeep /health." });
    expect(created.kind).toBe("created");
    expect(lastArgs("create_task")).toEqual({ project_id: "p1", title: "Add rate limiting", description: "Per IP.\nKeep /health." });
    await getTask("t1");
    expect(lastArgs("get_task")).toEqual({ id: "t1" });
    await updateTask({ id: "t1", revision: 1, title: "Add rate limiting", description: "", status: "archived" });
    expect(lastArgs("update_task")).toEqual({ id: "t1", revision: 1, title: "Add rate limiting", description: "", status: "archived" });
    expect(lastArgs("list_tasks") ?? {}).toEqual({});
  });

  it("refuses input Rust would refuse, without calling it", async () => {
    native();
    const invalid = { kind: "error", code: "invalid_input" };
    for (const id of ["", "t0", "T1", "p1", "../tasks.json", "/etc/hosts"]) expect(await getTask(id)).toEqual(invalid);
    for (const input of [
      { projectId: "../projects.json", title: "Title", description: "" },
      { projectId: "p1", title: "", description: "" },
      { projectId: "p1", title: " Title", description: "" },
      { projectId: "p1", title: "é".repeat(129), description: "" },
      { projectId: "p1", title: "Tab\there", description: "" },
      { projectId: "p1", title: "Title", description: "a\rb" },
      { projectId: "p1", title: "Title", description: "x".repeat(4096 - 2 - 5 + 1) },
    ]) {
      expect(await createTask(input), JSON.stringify(input)).toEqual(invalid);
    }
    for (const change of [
      { id: "t1", revision: 0, title: "Title", description: "", status: "draft" as const },
      { id: "t1", revision: 1.5, title: "Title", description: "", status: "draft" as const },
      { id: "t1", revision: 1, title: "Title", description: "", status: "running" as unknown as "draft" },
    ]) {
      expect(await updateTask(change)).toEqual(invalid);
    }
    expect(calls.filter((c) => c.cmd !== "list_projects")).toEqual([]);
  });

  it("counts limits in UTF-8 bytes, inclusively, and never cuts text", () => {
    expect(textProblem("é".repeat(128), "")).toBeNull();
    expect(textProblem("é".repeat(129), "")).toBe("title_too_long");
    expect(textProblem("Title", "x".repeat(4096 - 2 - 5))).toBeNull();
    expect(textProblem("Title", "x".repeat(4096 - 2 - 5 + 1))).toBe("too_long");
    expect(textProblem("Title", "line one\n\tindented")).toBeNull();
    expect(textProblem("Title", "carriage\rreturn")).toBe("description_control");
    expect(textProblem("Bell\u0007", "")).toBe("title_control");
    expect(textProblem("Two\nlines", "")).toBe("title_control");
    expect(textProblem("", "")).toBe("title_empty");
  });

  it("keeps only a known error code and drops the native message", async () => {
    native();
    taskFailure = "tasks_corrupt";
    expect(await listTasks()).toEqual({ kind: "error", code: "tasks_corrupt" });
    taskFailure = "something_new";
    expect(await listTasks()).toEqual({ kind: "unreadable" });
  });

  it("refuses an answer outside the contract instead of showing part of it", () => {
    const task = { id: "t1", title: "Title", status: "draft", project: { id: "p1", registered_at_ms: 1, link: "linked" }, created_at_ms: 1, updated_at_ms: 2, revision: 1 };
    expect(readList({ schema_version: 1, tasks: [task] })).toEqual({ kind: "ok", tasks: [task] });
    for (const bad of [
      { schema_version: 2, tasks: [task] },
      { schema_version: 1, tasks: [task], extra: true },
      { schema_version: 1, tasks: [{ ...task, description: "a list never carries one" }] },
      { schema_version: 1, tasks: [{ ...task, status: "running" }] },
      { schema_version: 1, tasks: [{ ...task, status: "completed" }] },
      { schema_version: 1, tasks: [{ ...task, project: { id: "p1", registered_at_ms: 1, link: "rebound" } }] },
      { schema_version: 1, tasks: [{ ...task, project: { id: "p1", registered_at_ms: 1, link: "linked", path: "/Users/me/x" } }] },
      // The registration time is part of the identity: without it, or as anything but a time, nothing is shown.
      { schema_version: 1, tasks: [{ ...task, project: { id: "p1", link: "linked" } }] },
      { schema_version: 1, tasks: [{ ...task, project: { id: "p1", registered_at_ms: "1", link: "linked" } }] },
      { schema_version: 1, tasks: [{ ...task, project: { id: "p1", registered_at_ms: -1, link: "linked" } }] },
      { schema_version: 1, tasks: [{ ...task, created_at_ms: 3 }] },
      { schema_version: 1, tasks: [{ ...task, revision: 0 }] },
      { schema_version: 1, tasks: [{ ...task, id: "x1" }] },
      { schema_version: 1, tasks: [task, task] },
    ]) {
      expect(readList(bad), JSON.stringify(bad)).toEqual({ kind: "unreadable" });
    }
    expect(readUpdate({ outcome: "conflict" })).toEqual({ kind: "unreadable" });
    expect(readUpdate({ outcome: "deleted" })).toEqual({ kind: "unreadable" });
    expect(readUpdate({ outcome: "updated", task })).toEqual({ kind: "unreadable" }); // no description
    expect(readUpdate({ outcome: "updated", task: { ...task, description: "" } }).kind).toBe("updated");
  });
});

describe("task ownership (#98)", () => {
  it("belongs to a registration only with the same id, the same registration time, and a link Rust confirmed", () => {
    const task = (project: TaskSummary["project"]): TaskSummary => ({
      id: "t1", title: "T", status: "draft", project, created_at_ms: 1, updated_at_ms: 1, revision: 1,
    });
    const p1 = { id: "p1", registered_at_ms: 100 };
    expect(belongsTo(task({ id: "p1", registered_at_ms: 100, link: "linked" }), p1)).toBe(true);
    expect(belongsTo(task({ id: "p1", registered_at_ms: 99, link: "linked" }), p1)).toBe(false);
    expect(belongsTo(task({ id: "p2", registered_at_ms: 100, link: "linked" }), p1)).toBe(false);
    // The interface's project list can be older than Rust's answer: Rust's link decides.
    for (const link of ["removed", "replaced", "unknown"] as const) {
      expect(belongsTo(task({ id: "p1", registered_at_ms: 100, link }), p1), link).toBe(false);
    }
  });
});

describe("Tasks page", () => {
  it("shows loading, then an empty state that offers New task, and never lists sample tasks", async () => {
    native();
    renderApp();
    expect(await main().findByRole("heading", { name: "No saved tasks yet" })).toBeInTheDocument();
    expect(main().getAllByRole("link", { name: "New task" })[0]).toHaveAttribute("href", "#/tasks/new");
    // Sample data is on, but fictional tasks are not presented as saved ones.
    expect(main().queryByText("Move auth to server-side sessions")).not.toBeInTheDocument();
    expect(screen.queryByRole("note", { name: "Sample data notice" })).not.toBeInTheDocument();
  });

  it("lists drafts with their project and date, and switches to archived tasks", async () => {
    const user = userEvent.setup();
    native();
    seed("Add rate limiting");
    seed("Old idea", { status: "archived" });
    renderApp();
    const drafts = await main().findByRole("list", { name: "Drafts" });
    const row = within(drafts).getByRole("link", { name: /Add rate limiting/ });
    expect(row).toHaveAttribute("href", "#/tasks/t1");
    expect(row).toHaveTextContent("atlas-api");
    expect(row).toHaveTextContent(/updated /);
    expect(within(row).getByText("Draft")).toBeInTheDocument();
    expect(within(drafts).queryByText("Old idea")).not.toBeInTheDocument();
    await user.click(main().getByRole("button", { name: /Archived/ }));
    const archived = main().getByRole("list", { name: "Archived tasks" });
    expect(within(archived).getByRole("link", { name: /Old idea/ })).toHaveTextContent("Archived");
    expect(main().getByRole("button", { name: /Archived/ })).toHaveAttribute("aria-pressed", "true");
  });

  it("names each project link state in words and never shows another project's name", async () => {
    native();
    // p2 was given to a different folder after a reset: the task was saved for the earlier p2.
    projects = [{ id: "p1", name: "atlas-api" }, { id: "p2", name: "new-folder", registered_at_ms: 2 }];
    seed("Linked", { project: { id: "p1", registered_at_ms: 1 } });
    seed("Removed", { project: { id: "p9", registered_at_ms: 1 } });
    seed("Replaced", { project: { id: "p2", registered_at_ms: 1 } });
    renderApp();
    const list = await main().findByRole("list", { name: "Drafts" });
    expect(within(list).getByRole("link", { name: /Linked/ })).toHaveTextContent("atlas-api");
    expect(within(list).getByRole("link", { name: /Removed/ })).toHaveTextContent("Project removed");
    const replaced = within(list).getByRole("link", { name: /Replaced/ });
    expect(replaced).toHaveTextContent("Project changed");
    expect(replaced).not.toHaveTextContent("new-folder");
  });

  it("shows an unreadable registry as Project unavailable", async () => {
    native();
    seed("Somewhere");
    links = { p1: "unknown" };
    renderApp();
    expect(await main().findByRole("link", { name: /Somewhere/ })).toHaveTextContent("Project unavailable");
  });

  it("shows corrupt storage as an error that says the file was kept, never as an empty list, and offers no reset", async () => {
    const user = userEvent.setup();
    native();
    seed("Kept safe");
    taskFailure = "tasks_corrupt";
    renderApp("#/tasks", false);
    const alert = await main().findByRole("alert");
    expect(alert).toHaveTextContent("kept exactly as it was");
    expect(alert).not.toHaveTextContent("Fixed native message");
    expect(main().queryByText("No saved tasks yet")).not.toBeInTheDocument();
    expect(main().queryByRole("button", { name: /reset|repair|recover|delete/i })).not.toBeInTheDocument();
    // Projects stay usable.
    await user.click(rail().getByRole("link", { name: "Projects" }));
    expect(await screen.findByRole("button", { name: /atlas-api/ })).toBeInTheDocument();
    // Retrying reads the list again once it can be read.
    await user.click(rail().getByRole("link", { name: /^Tasks/ }));
    taskFailure = null;
    await user.click(await main().findByRole("button", { name: "Try again" }));
    expect(await main().findByRole("link", { name: /Kept safe/ })).toBeInTheDocument();
  });

  it("explains a task file from a newer version", async () => {
    native();
    taskFailure = "tasks_newer_version";
    renderApp();
    expect(await main().findByRole("alert")).toHaveTextContent("newer version of Ordelis");
  });

  it("makes no native call in a browser preview and keeps New task unavailable with a reason", async () => {
    mockIPC(() => {
      throw new Error("must not be called");
    });
    renderApp();
    expect(await main().findByRole("heading", { name: "Saved tasks need the desktop app" })).toBeInTheDocument();
    const button = main().getByRole("button", { name: "New task" });
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(button).toHaveAccessibleDescription(/desktop app/);
  });

  it("renders titles as text, isolated in direction, never as markup", async () => {
    native();
    seed('<img src=x onerror="alert(1)"> **bold** https://example.com');
    renderApp();
    const row = await main().findByRole("link", { name: /onerror/ });
    expect(row.querySelector("img")).toBeNull();
    expect(row.querySelector("a[href^='https']")).toBeNull();
    const title = within(row).getByText(/onerror/);
    expect(title).toHaveAttribute("dir", "auto");
    expect(title.textContent).toBe('<img src=x onerror="alert(1)"> **bold** https://example.com');
  });

  it("loads once under React Strict Mode", async () => {
    native();
    window.location.hash = "#/tasks";
    render(
      <StrictMode>
        <SampleDataProvider initial={false}>
          <Shell />
        </SampleDataProvider>
      </StrictMode>,
    );
    await main().findByRole("heading", { name: "No saved tasks yet" });
    expect(count("list_tasks")).toBe(1);
  });
});

describe("New task", () => {
  async function fill(user: ReturnType<typeof userEvent.setup>, title: string, description = "") {
    await user.selectOptions(await main().findByRole("combobox", { name: "Project" }), "p1");
    await user.type(main().getByRole("textbox", { name: /Title/ }), title);
    if (description) await user.type(main().getByRole("textbox", { name: /Description/ }), description);
  }

  it("asks for a project first when none is registered", async () => {
    native();
    projects = [];
    renderApp("#/tasks/new");
    expect(await main().findByText(/Every task belongs to one of your project folders/)).toBeInTheDocument();
    expect(main().getByRole("link", { name: "Go to Projects" })).toHaveAttribute("href", "#/projects");
    expect(main().getByRole("button", { name: "Save task" })).toHaveAccessibleDescription("Add a project first.");
  });

  it("saves a draft through create_task, opens it, and finds it again after a restart", async () => {
    const user = userEvent.setup();
    native();
    const { unmount } = renderApp("#/tasks/new");
    await fill(user, "  Add rate limiting  ", "Per IP.{Enter}Keep the health check.");
    await user.click(main().getByRole("button", { name: "Save task" }));
    expect(await main().findByRole("heading", { level: 1, name: "Add rate limiting" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/tasks/t1");
    expect(lastArgs("create_task")).toEqual({ project_id: "p1", title: "Add rate limiting", description: "Per IP.\nKeep the health check." });
    expect(count("create_task")).toBe(1);
    unmount();

    // A complete restart: nothing is held in the page; the task comes back from the native store.
    renderApp("#/tasks");
    expect(await main().findByRole("link", { name: /Add rate limiting/ })).toHaveTextContent("atlas-api");
  });

  it("explains each limit in words and never cuts the text", async () => {
    const user = userEvent.setup();
    native();
    renderApp("#/tasks/new");
    await user.selectOptions(await main().findByRole("combobox", { name: "Project" }), "p1");
    await user.click(main().getByRole("button", { name: "Save task" }));
    expect(main().getByRole("alert")).toHaveTextContent("Give the task a title.");
    const title = main().getByRole("textbox", { name: /Title/ });
    await user.type(title, "é".repeat(129));
    expect(title).toHaveValue("é".repeat(129));
    expect(main().getByRole("alert")).toHaveTextContent("The title is too long");
    expect(title).toHaveAttribute("aria-invalid", "true");
    expect(main().getByText("258 / 256 bytes")).toBeInTheDocument();
    await user.clear(title);
    await user.type(title, "Fine");
    const description = main().getByRole("textbox", { name: /Description/ });
    await user.click(description);
    await user.paste("x".repeat(4096));
    expect(main().getByRole("alert")).toHaveTextContent("must fit in 4,094 bytes");
    expect(main().getByText("4,100 / 4,094 bytes with the title")).toBeInTheDocument();
    expect(description).toHaveValue("x".repeat(4096));
    expect(count("create_task")).toBe(0);
  });

  it("sends one request however often Save is pressed", async () => {
    const user = userEvent.setup();
    native();
    holdCreate = [];
    renderApp("#/tasks/new");
    await fill(user, "Once");
    await user.click(main().getByRole("button", { name: "Save task" }));
    await user.click(main().getByRole("button", { name: /Sav/ }));
    expect(count("create_task")).toBe(1);
    await act(async () => holdCreate!.forEach((release) => release(undefined)));
    expect(await main().findByRole("heading", { level: 1, name: "Once" })).toBeInTheDocument();
  });

  // The button is disabled while saving; the state itself also shares one request, so two saves started in the same
  // moment (before any re-render) still create one task.
  it("shares one request between saves started at the same moment", async () => {
    native();
    holdCreate = [];
    let state: ReturnType<typeof useTasks> | null = null;
    function Probe() {
      state = useTasks();
      return null;
    }
    render(
      <SampleDataProvider initial={false}>
        <WorkspaceProvider>
          <RegistryProvider>
            <TasksProvider>
              <Probe />
            </TasksProvider>
          </RegistryProvider>
        </WorkspaceProvider>
      </SampleDataProvider>,
    );
    await waitFor(() => expect(state!.loading).toBe(false));
    const input = { projectId: "p1", title: "Once", description: "" };
    let first!: Promise<unknown>, second!: Promise<unknown>;
    act(() => {
      first = state!.create(input);
      second = state!.create(input);
    });
    expect(count("create_task")).toBe(1);
    await act(async () => holdCreate!.forEach((release) => release(undefined)));
    expect(await first).toEqual(await second);
    expect(tasks).toHaveLength(1);
  });

  it("keeps everything typed when saving fails", async () => {
    const user = userEvent.setup();
    native();
    renderApp("#/tasks/new");
    await fill(user, "Will fail", "Details");
    projects = []; // removed meanwhile, as another window could
    await user.click(main().getByRole("button", { name: "Save task" }));
    expect(await main().findByRole("alert")).toHaveTextContent("no longer registered");
    expect(main().getByRole("textbox", { name: /Title/ })).toHaveValue("Will fail");
    expect(main().getByRole("textbox", { name: /Description/ })).toHaveValue("Details");
  });

  it("keeps Draft plan and the later steps unavailable, and says why", async () => {
    native();
    renderApp("#/tasks/new");
    const plan = await main().findByRole("button", { name: "Draft plan" });
    expect(plan).toHaveAttribute("aria-disabled", "true");
    expect(plan).toHaveAccessibleDescription(/Planning isn't available yet/);
    const phases = main().getByRole("list", { name: "Task phases" });
    expect(phases).toHaveTextContent("Review plan (not available yet)");
    expect(within(phases).getByText("1").closest("li")).toHaveAttribute("aria-current", "step");
  });

  it("keeps the draft while the user moves around the app", async () => {
    const user = userEvent.setup();
    native();
    renderApp("#/tasks/new");
    await fill(user, "Half written", "More later");
    await user.click(rail().getByRole("link", { name: "Projects" }));
    await screen.findByRole("heading", { level: 1, name: "Projects" });
    await user.click(rail().getByRole("link", { name: "New Task" }));
    expect(await main().findByRole("textbox", { name: /Title/ })).toHaveValue("Half written");
    expect(main().getByRole("textbox", { name: /Description/ })).toHaveValue("More later");
  });
});

describe("Task detail", () => {
  it("shows the saved text as written, with its project, status and dates", async () => {
    native();
    seed("Add rate limiting", { description: "Line one\n  Line two\n\n<b>not bold</b>" });
    renderApp("#/tasks/t1");
    expect(await main().findByRole("heading", { level: 1, name: "Add rate limiting" })).toHaveAttribute("dir", "auto");
    const description = main().getByText(/Line one/);
    expect(description.textContent).toBe("Line one\n  Line two\n\n<b>not bold</b>");
    expect(description.querySelector("b")).toBeNull();
    expect(main().getByText("Draft")).toBeInTheDocument();
    expect(main().getByRole("navigation", { name: "Breadcrumb" })).toHaveTextContent("atlas-api›Tasks");
    expect(within(main().getByRole("navigation", { name: "Breadcrumb" })).getByRole("link", { name: "atlas-api" })).toHaveAttribute(
      "href",
      "#/projects/p1",
    );
    expect(main().getByText("Created")).toBeInTheDocument();
    expect(main().getByText("Updated")).toBeInTheDocument();
    const plan = main().getByRole("button", { name: "Draft plan" });
    expect(plan).toHaveAttribute("aria-disabled", "true");
  });

  it("edits with the revision it started from and shows the saved result", async () => {
    const user = userEvent.setup();
    native();
    seed("Old title", { description: "Old body" });
    renderApp("#/tasks/t1");
    await user.click(await main().findByRole("button", { name: "Edit" }));
    const title = main().getByRole("textbox", { name: /Title/ });
    expect(title).toHaveFocus();
    await user.clear(title);
    await user.type(title, "New title");
    await user.click(main().getByRole("button", { name: "Save changes" }));
    expect(await main().findByRole("heading", { level: 1, name: "New title" })).toBeInTheDocument();
    expect(lastArgs("update_task")).toEqual({ id: "t1", revision: 1, title: "New title", description: "Old body", status: "draft" });
    expect(tasks[0].revision).toBe(2);
    expect(main().getByRole("button", { name: "Edit" })).toHaveFocus();
  });

  it("never overwrites a newer version: it shows it and keeps the user's edits for review", async () => {
    const user = userEvent.setup();
    native();
    seed("Original", { description: "Original body" });
    renderApp("#/tasks/t1");
    await user.click(await main().findByRole("button", { name: "Edit" }));
    const title = main().getByRole("textbox", { name: /Title/ });
    await user.clear(title);
    await user.type(title, "Mine");
    changedElsewhere("t1", { title: "Theirs", description: "Their body" });

    await user.click(main().getByRole("button", { name: "Save changes" }));
    expect(await main().findByRole("alert")).toHaveTextContent("changed elsewhere");
    expect(tasks[0]).toMatchObject({ title: "Theirs", revision: 2 });
    expect(main().getByRole("heading", { level: 1, name: "Theirs" })).toBeInTheDocument();
    expect(main().getByText("Their body")).toBeInTheDocument();
    expect(main().getByRole("textbox", { name: /Title/ })).toHaveValue("Mine");

    // After review, the user may deliberately save over the newer version.
    await user.click(main().getByRole("button", { name: "Save my version over it" }));
    expect(await main().findByRole("heading", { level: 1, name: "Mine" })).toBeInTheDocument();
    expect(lastArgs("update_task")).toMatchObject({ revision: 2, title: "Mine" });
  });

  it("discards edits only when asked, and then shows the latest version", async () => {
    const user = userEvent.setup();
    native();
    seed("Original");
    renderApp("#/tasks/t1");
    await user.click(await main().findByRole("button", { name: "Edit" }));
    await user.type(main().getByRole("textbox", { name: /Title/ }), " plus mine");
    changedElsewhere("t1", { title: "Theirs" });
    await user.click(main().getByRole("button", { name: "Save changes" }));
    await main().findByRole("alert");
    await user.click(main().getByRole("button", { name: "Discard edits" }));
    expect(main().queryByRole("textbox", { name: /Title/ })).not.toBeInTheDocument();
    expect(main().getByRole("heading", { level: 1, name: "Theirs" })).toBeInTheDocument();
  });

  it("archives and restores as ordinary revisioned updates", async () => {
    const user = userEvent.setup();
    native();
    seed("Park it");
    renderApp("#/tasks/t1");
    await user.click(await main().findByRole("button", { name: "Archive" }));
    expect(await main().findByText("Archived")).toBeInTheDocument();
    expect(lastArgs("update_task")).toMatchObject({ revision: 1, status: "archived", title: "Park it" });
    await user.click(main().getByRole("button", { name: "Restore" }));
    expect(await main().findByText("Draft")).toBeInTheDocument();
    expect(lastArgs("update_task")).toMatchObject({ revision: 2, status: "draft" });
  });

  it("does not archive over a newer version", async () => {
    const user = userEvent.setup();
    native();
    seed("Original");
    renderApp("#/tasks/t1");
    await main().findByRole("button", { name: "Archive" });
    changedElsewhere("t1", { title: "Renamed elsewhere" });
    await user.click(main().getByRole("button", { name: "Archive" }));
    expect(await main().findByRole("alert")).toHaveTextContent("nothing was saved");
    expect(tasks[0].status).toBe("draft");
    expect(main().getByRole("heading", { level: 1, name: "Renamed elsewhere" })).toBeInTheDocument();
  });

  it("keeps a task whose project was removed, says so, and links to no project", async () => {
    native();
    seed("Orphan", { project: { id: "p7", registered_at_ms: 1 }, description: "Still here" });
    renderApp("#/tasks/t1");
    expect(await main().findByRole("heading", { level: 1, name: "Orphan" })).toBeInTheDocument();
    expect(main().getByText("Still here")).toBeInTheDocument();
    const crumbs = main().getByRole("navigation", { name: "Breadcrumb" });
    expect(crumbs).toHaveTextContent("Project removed");
    expect(within(crumbs).queryByRole("link", { name: "Project removed" })).not.toBeInTheDocument();
    expect(main().getByText(/is not moved to another project/)).toBeInTheDocument();
  });

  it("says when a task does not exist", async () => {
    native();
    renderApp("#/tasks/t42");
    expect(await main().findByRole("heading", { name: "Task not found" })).toBeInTheDocument();
  });

  it("keeps unsaved edits while the user moves around the app", async () => {
    const user = userEvent.setup();
    native();
    seed("Original");
    renderApp("#/tasks/t1");
    await user.click(await main().findByRole("button", { name: "Edit" }));
    await user.type(main().getByRole("textbox", { name: /Description/ }), "Unsaved thought");
    await user.click(rail().getByRole("link", { name: /^Tasks/ }));
    await main().findByRole("list", { name: "Drafts" });
    await user.click(main().getByRole("link", { name: /Original/ }));
    expect(await main().findByRole("textbox", { name: /Description/ })).toHaveValue("Unsaved thought");
  });
});

describe("navigation and keyboard", () => {
  it("opens Tasks with Ctrl+2, moves focus to the page, and opens a task with Enter", async () => {
    const user = userEvent.setup();
    native();
    seed("Keyboard task");
    renderApp("#/home");
    await user.keyboard("{Control>}2{/Control}");
    expect(await main().findByRole("heading", { level: 1, name: "Tasks" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveFocus();
    expect(rail().getByRole("link", { name: /^Tasks/ })).toHaveAttribute("aria-current", "page");
    const row = await main().findByRole("link", { name: /Keyboard task/ });
    row.focus();
    await user.keyboard("{Enter}");
    expect(await main().findByRole("heading", { level: 1, name: "Keyboard task" })).toBeInTheDocument();
    // The task detail still marks Tasks in the sidebar.
    await waitFor(() => expect(rail().getByRole("link", { name: /^Tasks/ })).toHaveAttribute("aria-current", "page"));
  });

  it("never names an execution state on any task page", async () => {
    native();
    seed("A draft");
    seed("An archived one", { status: "archived" });
    for (const hash of ["#/tasks", "#/tasks/new", "#/tasks/t1", "#/tasks/t2"]) {
      const { unmount } = renderApp(hash, false);
      await waitFor(() => expect(count("list_tasks")).toBeGreaterThan(0));
      await screen.findByRole("heading", { level: 1 });
      expect(screen.getByRole("main").textContent, hash).not.toMatch(/\b(Running|Completed|Failed|Awaiting approval|Paused|Blocked)\b/);
      unmount();
    }
  });
});

// D3.1d (#98): Home's composer, project pages and the sidebar on the same saved tasks.
describe("Home composer", () => {
  const home = () => within(screen.getByRole("form", { name: "New task" }));
  async function fillHome(user: ReturnType<typeof userEvent.setup>, title: string, description = "", project = "atlas-api") {
    await screen.findByRole("option", { name: project });
    await user.selectOptions(home().getByRole("combobox", { name: "Project" }), project);
    await user.type(home().getByRole("textbox", { name: /^Title/ }), title);
    if (description) await user.type(home().getByRole("textbox", { name: /^Description/ }), description);
  }

  it("saves a real task and then finds it under Tasks, on Home, in its project and in the sidebar", async () => {
    const user = userEvent.setup();
    native();
    renderApp("#/home", false);
    expect(await main().findByText("No saved tasks yet. Write one above.")).toBeInTheDocument();
    await fillHome(user, "Rate limit the API", "Per IP.{Enter}Keep /health exempt.");
    await user.click(home().getByRole("button", { name: "Save task" }));

    expect(await main().findByRole("heading", { level: 1, name: "Rate limit the API" })).toBeInTheDocument();
    expect(calls.filter((c) => c.cmd === "create_task").map((c) => c.args)).toEqual([
      { project_id: "p1", title: "Rate limit the API", description: "Per IP.\nKeep /health exempt." },
    ]);
    expect(screen.getByRole("status")).toHaveTextContent("Task saved as a draft. Nothing was started.");
    expect(rail().getByRole("link", { name: /^Tasks, 1 draft$/ })).toBeInTheDocument();
    expect(rail().getByRole("link", { name: /^atlas-api, 1 draft$/ })).toBeInTheDocument();

    await user.click(rail().getByRole("link", { name: "Home" }));
    const drafts = await main().findByRole("list", { name: "Recent drafts" });
    expect(within(drafts).getByRole("link", { name: /Rate limit the API/ })).toHaveTextContent("atlas-api");
    expect(home().getByRole("textbox", { name: /^Title/ })).toHaveValue("");
    expect(within(main().getByRole("region", { name: "Recent projects" })).getByText("1 draft")).toBeInTheDocument();

    await user.click(rail().getByRole("link", { name: /^atlas-api/ }));
    const section = await main().findByRole("region", { name: "Tasks" });
    expect(within(section).getByText("1 draft")).toBeInTheDocument();
    expect(within(section).getByRole("list", { name: "Drafts in atlas-api" })).toHaveTextContent("Rate limit the API");

    await user.keyboard("{Control>}2{/Control}");
    expect(await main().findByRole("list", { name: "Drafts" })).toHaveTextContent("Rate limit the API");
  });

  it("saves from the keyboard: Enter in the title submits", async () => {
    const user = userEvent.setup();
    native();
    renderApp("#/home", false);
    await fillHome(user, "By keyboard");
    await user.keyboard("{Enter}");
    expect(await main().findByRole("heading", { level: 1, name: "By keyboard" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveFocus();
  });

  it("shares one draft with New task, and a save from either clears it once", async () => {
    const user = userEvent.setup();
    native();
    renderApp("#/home", false);
    await fillHome(user, "Shared", "Written on Home");
    await user.click(rail().getByRole("link", { name: "New Task" }));
    const form = within(await screen.findByRole("form", { name: "Prepare task" }));
    expect(form.getByRole("combobox", { name: "Project" })).toHaveValue("p1");
    expect(form.getByRole("textbox", { name: /^Title/ })).toHaveValue("Shared");
    expect(form.getByRole("textbox", { name: /^Description/ })).toHaveValue("Written on Home");
    await user.click(form.getByRole("button", { name: "Save task" }));
    await main().findByRole("heading", { level: 1, name: "Shared" });
    expect(count("create_task")).toBe(1);
    await user.click(rail().getByRole("link", { name: "Home" }));
    expect(home().getByRole("textbox", { name: /^Title/ })).toHaveValue("");
    expect(home().getByRole("textbox", { name: /^Description/ })).toHaveValue("");
    expect(home().getByRole("combobox", { name: "Project" })).toHaveValue("");
  });

  it("sends one request across Home and New task, and never pulls the user back when it finishes", async () => {
    const user = userEvent.setup();
    native();
    holdCreate = [];
    renderApp("#/home", false);
    await fillHome(user, "Only once");
    await user.click(home().getByRole("button", { name: "Save task" }));
    const saving = home().getByRole("button", { name: "Saving…" });
    expect(saving).toHaveAttribute("aria-disabled", "true");
    await user.click(saving);
    expect(home().getByRole("textbox", { name: /^Title/ })).toHaveAttribute("readonly");

    // The user moves on while the save is in flight; New task shows the same draft, and cannot save it again.
    await user.click(rail().getByRole("link", { name: "New Task" }));
    const form = within(await screen.findByRole("form", { name: "Prepare task" }));
    expect(form.getByRole("button", { name: "Saving…" })).toHaveAccessibleDescription("Saving…");
    await user.click(form.getByRole("button", { name: "Saving…" }));
    expect(count("create_task")).toBe(1);

    await act(async () => holdCreate!.forEach((release) => release(undefined)));
    await waitFor(() => expect(form.getByRole("textbox", { name: /^Title/ })).toHaveValue(""));
    expect(window.location.hash).toBe("#/tasks/new");
    expect(count("create_task")).toBe(1);
    expect(tasks.map((t) => t.title)).toEqual(["Only once"]);
  });

  it("keeps everything typed when saving fails, and explains limits without cutting the text", async () => {
    const user = userEvent.setup();
    native();
    renderApp("#/home", false);
    await fillHome(user, "é".repeat(129), "Body");
    await user.click(home().getByRole("button", { name: "Save task" }));
    expect(home().getByRole("alert")).toHaveTextContent("The title is too long");
    expect(home().getByRole("textbox", { name: /^Title/ })).toHaveValue("é".repeat(129));
    expect(home().getByRole("textbox", { name: /^Title/ })).toHaveAccessibleName(/258 \/ 256 bytes/);
    expect(count("create_task")).toBe(0);

    await user.clear(home().getByRole("textbox", { name: /^Title/ }));
    await user.type(home().getByRole("textbox", { name: /^Title/ }), "Will fail");
    projects = []; // removed meanwhile, as another window could
    await user.click(home().getByRole("button", { name: "Save task" }));
    expect(await main().findByText(/no longer registered/)).toBeInTheDocument();
    expect(home().getByRole("textbox", { name: /^Title/ })).toHaveValue("Will fail");
    expect(home().getByRole("textbox", { name: /^Description/ })).toHaveValue("Body");
  });

  it("leaves New task with Back to Tasks without losing the draft, and discards only after confirming", async () => {
    const user = userEvent.setup();
    native();
    renderApp("#/tasks/new", false);
    await screen.findByRole("option", { name: "atlas-api" });
    await user.type(main().getByRole("textbox", { name: /^Title/ }), "Kept");
    expect(main().queryByRole("link", { name: "Cancel" })).not.toBeInTheDocument();
    await user.click(main().getByRole("link", { name: "Back to Tasks" }));
    await main().findByRole("heading", { level: 1, name: "Tasks" });
    await user.click(rail().getByRole("link", { name: "New Task" }));
    expect(await main().findByRole("textbox", { name: /^Title/ })).toHaveValue("Kept");

    await user.click(main().getByRole("button", { name: "Discard draft…" }));
    await user.click(within(screen.getByRole("alertdialog", { name: "Discard this draft?" })).getByRole("button", { name: "Discard" }));
    expect(main().getByRole("textbox", { name: /^Title/ })).toHaveValue("");
    expect(main().getByRole("textbox", { name: /^Title/ })).toHaveFocus();
    expect(count("create_task")).toBe(0);
  });
});

describe("project pages and counts", () => {
  it("counts only a registration's own tasks, drafts apart from archived, and follows archive and restore", async () => {
    const user = userEvent.setup();
    native();
    // p2 was given to a different folder after a reset; "Old p2" was saved for the earlier p2.
    projects = [{ id: "p1", name: "atlas-api" }, { id: "p2", name: "new-folder", registered_at_ms: 2 }];
    seed("Mine", { project: { id: "p1", registered_at_ms: 1 } });
    seed("Parked", { project: { id: "p1", registered_at_ms: 1 }, status: "archived" });
    seed("Old p2", { project: { id: "p2", registered_at_ms: 1 } });
    seed("Gone", { project: { id: "p9", registered_at_ms: 1 } });
    renderApp("#/projects/p2", false);

    const p2 = await main().findByRole("region", { name: "Tasks" });
    expect(await within(p2).findByText("No tasks saved for this project yet.")).toBeInTheDocument();
    expect(within(p2).getByText("0 drafts")).toBeInTheDocument();
    expect(main().queryByText("Old p2")).not.toBeInTheDocument();
    expect(rail().getByRole("link", { name: "new-folder" })).toBeInTheDocument();
    expect(rail().getByRole("link", { name: /^atlas-api, 1 draft$/ })).toBeInTheDocument();
    // The Tasks total is every saved draft, detached ones included: they are listed there with their link state.
    expect(rail().getByRole("link", { name: /^Tasks, 3 drafts$/ })).toBeInTheDocument();

    await user.click(rail().getByRole("link", { name: /^atlas-api/ }));
    const p1 = await main().findByRole("region", { name: "Tasks" });
    expect(within(p1).getByText("1 draft · 1 archived")).toBeInTheDocument();
    expect(within(p1).getByRole("list", { name: "Drafts in atlas-api" })).toHaveTextContent("Mine");
    expect(within(p1).getByRole("list", { name: "Archived tasks in atlas-api" })).toHaveTextContent("Parked");

    await user.click(within(p1).getByRole("link", { name: /Mine/ }));
    await user.click(await main().findByRole("button", { name: "Archive" }));
    await main().findByRole("button", { name: "Restore" });
    expect(rail().getByRole("link", { name: "atlas-api" })).toBeInTheDocument();
    expect(rail().getByRole("link", { name: /^Tasks, 2 drafts$/ })).toBeInTheDocument();
    await user.click(main().getByRole("link", { name: "atlas-api" }));
    expect(await within(await main().findByRole("region", { name: "Tasks" })).findByText("0 drafts · 2 archived")).toBeInTheDocument();
  });

  it("starts a task in the project, and checks the project again before saving", async () => {
    const user = userEvent.setup();
    native();
    projects = [{ id: "p1", name: "atlas-api" }, { id: "p2", name: "mochi-web" }];
    renderApp("#/projects/p2", false);
    await main().findByRole("heading", { level: 2, name: "mochi-web" });
    await user.click(await main().findByRole("button", { name: "New task in this project" }));
    const form = within(await screen.findByRole("form", { name: "Prepare task" }));
    expect(form.getByRole("combobox", { name: "Project" })).toHaveValue("p2");
    expect(screen.getByRole("status")).toHaveTextContent("New task in mochi-web.");
    await user.type(form.getByRole("textbox", { name: /^Title/ }), "For mochi");

    // The project is removed before the task is saved: the draft is not moved to any other project.
    await user.click(rail().getByRole("link", { name: "mochi-web" }));
    await user.click(await main().findByRole("button", { name: "Remove from Ordelis…" }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(rail().queryByRole("link", { name: "mochi-web" })).not.toBeInTheDocument());
    await user.click(rail().getByRole("link", { name: "New Task" }));
    const again = within(await screen.findByRole("form", { name: "Prepare task" }));
    expect(again.getByRole("combobox", { name: "Project" })).toHaveValue("");
    expect(again.getByRole("textbox", { name: /^Title/ })).toHaveValue("For mochi");
    await user.click(again.getByRole("button", { name: "Save task" }));
    expect(again.getByRole("alert")).toHaveTextContent("Choose the project this task belongs to.");
    expect(count("create_task")).toBe(0);
  });

  it("never moves a draft to a different registration that reuses the project's id", async () => {
    const user = userEvent.setup();
    native();
    projects = [{ id: "p1", name: "atlas-api" }, { id: "p2", name: "mochi-web" }];
    renderApp("#/projects/p2", false);
    await user.click(await main().findByRole("button", { name: "New task in this project" }));
    const form = within(await screen.findByRole("form", { name: "Prepare task" }));
    await user.type(form.getByRole("textbox", { name: /^Title/ }), "For the first mochi-web");
    // Elsewhere, the project list is reset and p2 now names another folder. The next list shows it.
    projects = [{ id: "p1", name: "atlas-api" }, { id: "p2", name: "other-folder", registered_at_ms: 2 }];
    await user.click(rail().getByRole("link", { name: "atlas-api" }));
    await user.click(await main().findByRole("button", { name: "Remove from Ordelis…" }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Remove" }));
    await screen.findByRole("link", { name: "other-folder" });
    await user.click(rail().getByRole("link", { name: "New Task" }));
    const again = within(await screen.findByRole("form", { name: "Prepare task" }));
    expect(again.getByRole("combobox", { name: "Project" })).toHaveValue("");
    await user.click(again.getByRole("button", { name: "Save task" }));
    expect(again.getByRole("alert")).toHaveTextContent("Choose the project this task belongs to.");
    expect(count("create_task")).toBe(0);
  });

  it("lists the tasks again when the project list changes, so a removed project reads as removed", async () => {
    const user = userEvent.setup();
    native();
    seed("Keep me");
    renderApp("#/projects/p1", false);
    await within(await main().findByRole("region", { name: "Tasks" })).findByText("1 draft");
    const before = count("list_tasks");
    await user.click(main().getByRole("button", { name: "Remove from Ordelis…" }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(count("list_tasks")).toBe(before + 1));
    await user.click(rail().getByRole("link", { name: /^Tasks/ }));
    expect(await main().findByRole("link", { name: /Keep me/ })).toHaveTextContent("Project removed");
    expect(tasks[0].project).toEqual({ id: "p1", registered_at_ms: 1 });
  });

  it("never shows a count it could not read: corrupt storage leaves Home, projects and the sidebar uncounted", async () => {
    const user = userEvent.setup();
    native();
    seed("Unreadable");
    taskFailure = "tasks_corrupt";
    renderApp("#/home", false);
    expect(await within(main().getByRole("region", { name: "Drafts" })).findByRole("alert")).toHaveTextContent(
      "The task file was kept exactly as it was",
    );
    expect(rail().getByRole("link", { name: "Tasks" })).toBeInTheDocument();
    expect(rail().getByRole("link", { name: "atlas-api" })).toBeInTheDocument();
    expect(main().queryByText(/\d+ drafts?/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /reset|recover|repair/i })).not.toBeInTheDocument();

    await user.click(rail().getByRole("link", { name: "atlas-api" }));
    const section = await main().findByRole("region", { name: "Tasks" });
    expect(await within(section).findByRole("alert")).toHaveTextContent("could not be read");
    expect(within(section).queryByText(/0 drafts/)).not.toBeInTheDocument();

    taskFailure = null;
    await user.click(within(section).getByRole("button", { name: "Try again" }));
    expect(await within(section).findByText("1 draft")).toBeInTheDocument();
    expect(rail().getByRole("link", { name: /^atlas-api, 1 draft$/ })).toBeInTheDocument();
  });
});

