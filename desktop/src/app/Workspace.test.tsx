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
const draftBox = () => screen.getByRole("textbox", { name: "Describe the task" });
const projectSelect = () => screen.getByRole("combobox", { name: "Project" });
const status = () => screen.getByRole("status");

describe("Home composer", () => {
  it("selects a project and announces it", async () => {
    const user = userEvent.setup();
    renderApp();
    expect(projectSelect()).toHaveValue("atlas-api");
    await user.selectOptions(projectSelect(), "mochi-web");
    expect(projectSelect()).toHaveValue("mochi-web");
    expect(status()).toHaveTextContent("Project mochi-web selected");
  });

  it("edits and explicitly clears the draft", async () => {
    const user = userEvent.setup();
    renderApp();
    expect(screen.queryByRole("button", { name: "Clear draft" })).not.toBeInTheDocument();
    await user.type(draftBox(), "fix the login bug");
    await user.click(screen.getByRole("button", { name: "Clear draft" }));
    expect(draftBox()).toHaveValue("");
    expect(status()).toHaveTextContent("Draft cleared");
  });

  it("updates the live region when the same action is announced twice", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.type(draftBox(), "first draft");
    await user.click(screen.getByRole("button", { name: "Clear draft" }));
    const mutations: MutationRecord[] = [];
    const observer = new MutationObserver((records) => mutations.push(...records));
    observer.observe(status(), { childList: true, subtree: true, characterData: true });

    await user.type(draftBox(), "second draft");
    await user.click(screen.getByRole("button", { name: "Clear draft" }));
    await Promise.resolve();
    observer.disconnect();

    expect(status()).toHaveTextContent("Draft cleared");
    expect(mutations.length).toBeGreaterThan(0);
  });

  it("keeps the draft and project across in-app navigation", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.type(draftBox(), "unfinished thought");
    await user.selectOptions(projectSelect(), "synapse");
    await user.click(rail().getByRole("link", { name: "Settings" }));
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    await user.click(rail().getByRole("link", { name: "Home" }));
    expect(draftBox()).toHaveValue("unfinished thought");
    expect(projectSelect()).toHaveValue("synapse");
  });

  it("selects sample context notes, inertly, and resets them on project change", async () => {
    const user = userEvent.setup();
    renderApp();
    const toggle = screen.getByRole("button", { name: "Select context" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const group = screen.getByRole("group", { name: /Sample context for atlas-api/ });
    await user.click(within(group).getByRole("checkbox", { name: /Security \/ Tokens/ }));
    expect(within(group).getByRole("checkbox", { name: /Security \/ Tokens/ })).toBeChecked();
    expect(screen.getByText("1 of 5 selected")).toBeInTheDocument();
    expect(status()).toHaveTextContent("Security / Tokens added to sample context");
    await user.selectOptions(projectSelect(), "synapse");
    expect(screen.queryByRole("group", { name: /Sample context/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Select context" })).toHaveAccessibleDescription(/no sample context/i);
  });

  it("offers only a preview action, disabled with a reason until there is a draft", async () => {
    const user = userEvent.setup();
    renderApp();
    expect(screen.queryByRole("button", { name: /^(start|run)/i })).not.toBeInTheDocument();
    const preview = screen.getByRole("button", { name: "Preview task" });
    expect(preview).toHaveAttribute("aria-disabled", "true");
    expect(preview).toHaveAccessibleDescription("Describe the task first.");
    await user.click(preview);
    expect(screen.getByRole("heading", { level: 1, name: "What should we work on?" })).toBeInTheDocument();

    await user.type(draftBox(), "line one{Enter}line two");
    await user.click(screen.getByRole("button", { name: "Preview task" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Task preview" })).toBeInTheDocument();
    expect(screen.getByText(/line one\s+line two/)).toBeInTheDocument();
    expect(screen.getByRole("note", { name: "Sample data notice" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Plan with agents" })).toHaveAttribute("aria-disabled", "true");
    expect(rail().getByRole("link", { name: "Home" })).toHaveAttribute("aria-current", "page");
    await user.click(screen.getByRole("button", { name: "Back to draft" }));
    expect(draftBox()).toHaveValue("line one\nline two");
  });

  it("has nothing to preview when no project is chosen", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.type(draftBox(), "something");
    await user.selectOptions(projectSelect(), "");
    expect(status()).toHaveTextContent("No project selected");
    expect(screen.getByRole("button", { name: "Preview task" })).toHaveAccessibleDescription("Choose a project first.");
    window.location.hash = "#/preview";
    expect(await screen.findByRole("heading", { name: "Nothing to preview" })).toBeInTheDocument();
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

  it("shows first-use guidance, keeps the draft and cannot preview when sample data is off", async () => {
    const user = userEvent.setup();
    renderApp(false);
    expect(screen.getByRole("heading", { name: "Set up Synapse" })).toBeInTheDocument();
    expect(projectSelect()).toBeDisabled();
    await user.type(draftBox(), "still typing");
    expect(screen.getByRole("button", { name: "Preview task" })).toHaveAccessibleDescription(/Sample data is hidden/);
    expect(screen.queryByRole("link", { name: /Fix pagination/ })).not.toBeInTheDocument();
  });

  it("does not restore stale project or context selections after sample data is hidden", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(screen.getByRole("button", { name: "Select context" }));
    await user.click(screen.getByRole("checkbox", { name: /Security \/ Tokens/ }));
    await user.click(screen.getByRole("button", { name: "Hide sample data" }));
    await user.click(rail().getByRole("link", { name: "Settings" }));
    await user.click(screen.getByRole("checkbox", { name: "Show sample data" }));
    await user.click(rail().getByRole("link", { name: "Home" }));
    expect(projectSelect()).toHaveValue("");
    await user.selectOptions(projectSelect(), "atlas-api");
    await user.click(screen.getByRole("button", { name: "Select context" }));

    expect(screen.getByRole("checkbox", { name: /Security \/ Tokens/ })).not.toBeChecked();
    expect(screen.getByText("5 available")).toBeInTheDocument();
  });
});

describe("Projects", () => {
  it("stays in sync with Home in both directions", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.selectOptions(projectSelect(), "mochi-web");
    await user.click(rail().getByRole("link", { name: "Projects" }));
    expect(await screen.findByRole("heading", { level: 2, name: "mochi-web" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /mochi-web/ })).toHaveAttribute("aria-pressed", "true");

    await user.click(screen.getByRole("button", { name: /synapse/ }));
    await user.click(rail().getByRole("link", { name: "Home" }));
    expect(projectSelect()).toHaveValue("synapse");
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

  it("'Draft task here' goes to the composer with the project already chosen", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(rail().getByRole("link", { name: "Projects" }));
    await user.click(await screen.findByRole("button", { name: /ensaios/ }));
    await user.click(screen.getByRole("button", { name: "Draft task here" }));
    expect(await screen.findByRole("heading", { level: 1, name: "What should we work on?" })).toBeInTheDocument();
    expect(projectSelect()).toHaveValue("ensaios");
  });

  it("opens a project from a link and selects it", async () => {
    renderApp();
    window.location.hash = "#/projects/mochi-web";
    expect(await screen.findByRole("heading", { level: 2, name: "mochi-web" })).toBeInTheDocument();
    window.location.hash = "#/home";
    expect(await screen.findByRole("combobox", { name: "Project" })).toHaveValue("mochi-web");
  });

  it("handles unselected, not-found and empty states", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.selectOptions(projectSelect(), "");
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

  it("keeps the six primary destinations and the shortcuts", async () => {
    const user = userEvent.setup();
    renderApp();
    expect(rail().getAllByRole("link").filter((l) => l.getAttribute("aria-label") !== "Synapse")).toHaveLength(6);
    await user.keyboard("{Control>}2{/Control}");
    expect(await screen.findByRole("heading", { level: 1, name: "Projects" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveFocus();
  });

  it("is operable from the keyboard alone", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(draftBox());
    await user.keyboard("by keyboard");
    await user.tab(); // project
    await user.tab(); // Clear draft
    expect(screen.getByRole("button", { name: "Clear draft" })).toHaveFocus();
    await user.tab(); // Select context
    await user.keyboard("{Enter}");
    expect(screen.getByRole("button", { name: "Select context" })).toHaveAttribute("aria-expanded", "true");
    await user.tab(); // Preview task
    await user.tab(); // first checkbox, which follows the toolbar in DOM order
    await user.keyboard(" ");
    expect(screen.getAllByRole("checkbox")[0]).toBeChecked();
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
    for (const f of files) {
      expect(readFileSync(f, "utf8"), f).not.toMatch(
        /@tauri-apps|__TAURI__|\binvoke\(|fetch\(|XMLHttpRequest|WebSocket|localStorage|sessionStorage|indexedDB|child_process|node:fs|process\.env/,
      );
    }
  });
});
