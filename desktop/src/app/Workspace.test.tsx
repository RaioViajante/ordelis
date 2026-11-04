import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SampleDataProvider } from "./SampleData";
import { Shell } from "./Shell";

function renderApp(sample = true) {
  return render(
    <SampleDataProvider initial={sample}>
      <Shell />
    </SampleDataProvider>,
  );
}

const rail = () => within(screen.getByRole("navigation", { name: "Main" }));
const home = () => within(screen.getByRole("form", { name: "New task" }));
const titleBox = () => home().getByRole("textbox", { name: /^Title/ });
const descriptionBox = () => home().getByRole("textbox", { name: /^Description/ });
const projectSelect = () => screen.getByRole("combobox", { name: "Project" });
const status = () => screen.getByRole("status");

// These run as a browser preview: no native layer, so nothing can be saved. Saving is covered in Tasks.test.tsx.
describe("Home composer in a browser preview", () => {
  it("is the real task composer, and cannot save or start anything here", async () => {
    renderApp();
    expect(screen.getByRole("heading", { level: 1, name: "What should we work on?" })).toBeInTheDocument();
    expect(projectSelect()).toBeDisabled();
    expect(projectSelect()).toHaveDisplayValue("No projects in the browser preview");
    const save = home().getByRole("button", { name: "Save task" });
    expect(save).toHaveAttribute("aria-disabled", "true");
    expect(save).toHaveAccessibleDescription("Saving tasks needs the desktop app.");
    expect(home().getByRole("button", { name: "Draft plan" })).toHaveAccessibleDescription(/Planning isn't available yet/);
    expect(home().getByRole("button", { name: "Context" })).toHaveAccessibleDescription(/isn't available yet/);
    expect(screen.queryByRole("button", { name: /Preview task|Select context|^(start|run)/i })).not.toBeInTheDocument();
    // No saved tasks can exist here, so Home lists none and counts none.
    expect(screen.queryByRole("heading", { name: "Drafts" })).not.toBeInTheDocument();
  });

  it("keeps the draft across in-app navigation and shares it with New task", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.type(titleBox(), "Unfinished");
    await user.type(descriptionBox(), "line one{Enter}line two");
    await user.click(rail().getByRole("link", { name: "Settings" }));
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    await user.click(rail().getByRole("link", { name: "New Task" }));
    const form = within(await screen.findByRole("form", { name: "Prepare task" }));
    expect(form.getByRole("textbox", { name: /^Title/ })).toHaveValue("Unfinished");
    await user.type(form.getByRole("textbox", { name: /^Description/ }), " and three");
    await user.click(rail().getByRole("link", { name: "Home" }));
    expect(titleBox()).toHaveValue("Unfinished");
    expect(descriptionBox()).toHaveValue("line one\nline two and three");
  });

  it("discards the draft only after the user confirms, and says so", async () => {
    const user = userEvent.setup();
    renderApp();
    expect(home().queryByRole("button", { name: "Discard draft…" })).not.toBeInTheDocument();
    await user.type(titleBox(), "Keep me");
    await user.click(home().getByRole("button", { name: "Discard draft…" }));
    const dialog = screen.getByRole("alertdialog", { name: "Discard this draft?" });
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(titleBox()).toHaveValue("Keep me");
    expect(home().getByRole("button", { name: "Discard draft…" })).toHaveFocus();

    await user.click(home().getByRole("button", { name: "Discard draft…" }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Discard" }));
    expect(titleBox()).toHaveValue("");
    expect(titleBox()).toHaveFocus();
    expect(status()).toHaveTextContent("Draft discarded.");
  });

  it("updates the live region when the same action is announced twice", async () => {
    const user = userEvent.setup();
    renderApp();
    const discard = async (text: string) => {
      await user.type(titleBox(), text);
      await user.click(home().getByRole("button", { name: "Discard draft…" }));
      await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Discard" }));
    };
    await discard("first");
    const mutations: MutationRecord[] = [];
    const observer = new MutationObserver((records) => mutations.push(...records));
    observer.observe(status(), { childList: true, subtree: true, characterData: true });
    await discard("second");
    await Promise.resolve();
    observer.disconnect();
    expect(status()).toHaveTextContent("Draft discarded.");
    expect(mutations.length).toBeGreaterThan(0);
  });

  it("opens a sample task preview from an active task row", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(screen.getByRole("link", { name: /Fix pagination on \/notes/ }));
    expect(await screen.findByRole("heading", { level: 1, name: "Fix pagination on /notes" })).toBeInTheDocument();
    expect(screen.getByText("sample, not live")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open terminals" })).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("main")).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Back to atlas-api" }));
    expect(await screen.findByRole("heading", { level: 2, name: "atlas-api" })).toBeInTheDocument();
  });

  it("restores preview and project detail routes through browser history", async () => {
    const user = userEvent.setup();
    window.history.replaceState(null, "", "#/home");
    renderApp();
    await user.click(screen.getByRole("link", { name: /Fix pagination on \/notes/ }));
    await user.click(await screen.findByRole("button", { name: "Back to atlas-api" }));
    expect(await screen.findByRole("heading", { level: 2, name: "atlas-api" })).toBeInTheDocument();

    window.history.back();
    expect(await screen.findByRole("heading", { level: 1, name: "Fix pagination on /notes" })).toBeInTheDocument();
    window.history.forward();
    await waitFor(() => expect(screen.getByRole("heading", { level: 2, name: "atlas-api" })).toBeInTheDocument());
  });

  it("shows first-use guidance and keeps the composer when sample data is off", async () => {
    const user = userEvent.setup();
    renderApp(false);
    expect(screen.getByRole("heading", { name: "Set up Ordelis" })).toBeInTheDocument();
    await user.type(titleBox(), "still typing");
    expect(titleBox()).toHaveValue("still typing");
    expect(home().getByRole("button", { name: "Save task" })).toHaveAccessibleDescription("Saving tasks needs the desktop app.");
    expect(screen.queryByRole("link", { name: /Fix pagination/ })).not.toBeInTheDocument();
  });
});

describe("Projects", () => {
  it("keeps the sample project selection in sync with Context", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(rail().getByRole("link", { name: "Projects" }));
    await user.click(await screen.findByRole("button", { name: /mochi-web/ }));
    await user.click(rail().getByRole("link", { name: "Context" }));
    expect(await screen.findByRole("combobox", { name: "Project" })).toHaveValue("mochi-web");
    await user.selectOptions(screen.getByRole("combobox", { name: "Project" }), "example-project");
    await user.click(rail().getByRole("link", { name: "Projects" }));
    expect(await screen.findByRole("heading", { level: 2, name: "example-project" })).toBeInTheDocument();
  });

  it("shows sample repository, workspace and task details with inert paths", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(rail().getByRole("link", { name: "Projects" }));
    expect(await screen.findByText("main @ 9b04e7d")).toBeInTheDocument();
    expect(screen.getAllByText("~/code/atlas-api").length).toBeGreaterThan(0);
    expect(screen.queryByRole("link", { name: /~\/code/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: /Speed up note search/ }));
    expect(await screen.findByRole("heading", { level: 1, name: "Speed up note search" })).toBeInTheDocument();
  });

  it("never starts a saved task in a sample project", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(rail().getByRole("link", { name: "Projects" }));
    await user.click(await screen.findByRole("button", { name: /ensaios/ }));
    const here = screen.getByRole("button", { name: "New task in this project" });
    expect(here).toHaveAttribute("aria-disabled", "true");
    expect(here).toHaveAccessibleDescription(/Saved tasks belong to your own folders/);
    await user.click(here);
    expect(screen.getByRole("heading", { level: 2, name: "ensaios" })).toBeInTheDocument();
  });

  it("opens a project from a link and selects it", async () => {
    const user = userEvent.setup();
    renderApp();
    window.location.hash = "#/projects/mochi-web";
    expect(await screen.findByRole("heading", { level: 2, name: "mochi-web" })).toBeInTheDocument();
    await user.click(rail().getByRole("link", { name: "Context" }));
    expect(await screen.findByRole("combobox", { name: "Project" })).toHaveValue("mochi-web");
  });

  it("handles unselected, not-found and empty states", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(rail().getByRole("link", { name: "Context" }));
    await user.selectOptions(await screen.findByRole("combobox", { name: "Project" }), "");
    await user.click(rail().getByRole("link", { name: "Projects" }));
    expect(await screen.findByRole("heading", { name: "No project selected" })).toBeInTheDocument();

    window.location.hash = "#/projects/nope";
    expect(await screen.findByRole("heading", { name: "Project not found" })).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: "Back to projects" }));
    expect(await screen.findByRole("heading", { name: "No project selected" })).toBeInTheDocument();

    window.location.hash = "#/task/nope";
    expect(await screen.findByRole("heading", { name: "Task not found" })).toBeInTheDocument();
  });

  it("treats a malformed encoded project id as not found", () => {
    window.history.replaceState(null, "", "#/projects/%");
    renderApp();
    expect(screen.getByRole("heading", { name: "Project not found" })).toBeInTheDocument();
  });

  it("shows the empty state, and no selection, when sample data is off", async () => {
    const user = userEvent.setup();
    renderApp(false);
    await user.click(rail().getByRole("link", { name: "Projects" }));
    expect(await screen.findByRole("heading", { name: "No projects yet" })).toBeInTheDocument();
    window.location.hash = "#/task/move-auth";
    expect(await screen.findByRole("heading", { name: "Task not found" })).toBeInTheDocument();
  });
});

describe("accessibility and boundaries", () => {
  it("announces through one polite live region", async () => {
    renderApp();
    expect(status()).toHaveAttribute("aria-live", "polite");
    expect(screen.getAllByRole("status")).toHaveLength(1);
  });

  it("keeps the seven primary destinations and the shortcuts", async () => {
    const user = userEvent.setup();
    renderApp();
    for (const name of ["Home", "Tasks", "Projects", "Sessions", "Context", "Agents", "Settings"]) {
      expect(rail().getByRole("link", { name })).toBeInTheDocument();
    }
    await user.keyboard("{Control>}4{/Control}");
    expect(await screen.findByRole("heading", { level: 1, name: "Projects" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveFocus();
  });

  it("is operable from the keyboard alone", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(titleBox());
    await user.keyboard("by keyboard");
    await user.tab();
    expect(descriptionBox()).toHaveFocus();
    await user.keyboard("and more");
    await user.tab(); // the project picker is disabled in a browser preview, so Context is next
    expect(home().getByRole("button", { name: "Context" })).toHaveFocus();
    await user.tab();
    expect(home().getByRole("button", { name: "Discard draft…" })).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(home().getByRole("button", { name: "Discard draft…" })).toHaveFocus();
    expect(descriptionBox()).toHaveValue("and more");
  });

  it("defines narrow-window rules for the new layouts", () => {
    const css = readFileSync(join(__dirname, "..", "styles", "index.css"), "utf8");
    expect(css).toMatch(/\.ask__bar\s*{[^}]*flex-wrap:\s*wrap/);
    expect(css).toMatch(/@media \(max-width: 1000px\)[\s\S]*?\.lay/);
    expect(css).toMatch(/\.preview-text\s*{[^}]*overflow-wrap:\s*anywhere/);
  });

  it("adds no privileged calls, storage, network or process access", () => {
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const name of readdirSync(dir)) {
        const p = join(dir, name);
        if (statSync(p).isDirectory()) walk(p);
        else if (/\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name)) files.push(p);
      }
    };
    walk(join(__dirname, ".."));
    expect(files.length).toBeGreaterThan(10);
    // The runtime status, project and task adapters are the reviewed exceptions for the Tauri API and invoke (see
    // config.test.ts).
    const adapters = [join(__dirname, "runtimeStatus.ts"), join(__dirname, "projects.ts"), join(__dirname, "tasks.ts")];
    for (const a of adapters) expect(files).toContain(a);
    for (const f of files) {
      const text = readFileSync(f, "utf8");
      expect(text, f).not.toMatch(
        /__TAURI|fetch\(|XMLHttpRequest|WebSocket|localStorage|sessionStorage|indexedDB|child_process|node:fs|process\.env/,
      );
      if (!adapters.includes(f)) expect(text, f).not.toMatch(/@tauri-apps|\binvoke\(/);
    }
  });
});
