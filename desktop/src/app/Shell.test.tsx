import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Button } from "../components/ui";
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

describe("navigation", () => {
  it("opens on Home and marks it current", () => {
    renderApp();
    expect(screen.getByRole("heading", { level: 1, name: "What should we work on?" })).toBeInTheDocument();
    expect(rail().getByRole("link", { name: "Home" })).toHaveAttribute("aria-current", "page");
  });

  it.each([
    ["Projects", "Projects"],
    ["Sessions", "Sessions"],
    ["Context", "Context"],
    ["Agents", "Agents"],
    ["Settings", "Settings"],
  ])("rail link %s opens its page", async (label, heading) => {
    const user = userEvent.setup();
    renderApp();
    await user.click(rail().getByRole("link", { name: label }));
    expect(await screen.findByRole("heading", { level: 1, name: heading })).toBeInTheDocument();
    expect(rail().getByRole("link", { name: label })).toHaveAttribute("aria-current", "page");
    expect(rail().getByRole("link", { name: "Home" })).not.toHaveAttribute("aria-current");
  });

  it("switches page with Ctrl+number and moves focus to the page", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.keyboard("{Control>}3{/Control}");
    expect(await screen.findByRole("heading", { level: 1, name: "Sessions" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveFocus();
  });

  it("ignores unmapped number shortcuts", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.keyboard("{Control>}9{/Control}");
    expect(screen.getByRole("heading", { level: 1, name: "What should we work on?" })).toBeInTheDocument();
  });

  it("reaches every rail destination by Tab and activates with Enter", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.tab(); // Synapse mark
    for (let i = 0; i < 4; i++) await user.tab(); // Home, Projects, Sessions, Context
    expect(rail().getByRole("link", { name: "Context" })).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { level: 1, name: "Context" })).toBeInTheDocument();
  });

});

describe("honesty of mock data", () => {
  it("labels sample data and lets the user hide it", async () => {
    const user = userEvent.setup();
    renderApp();
    const note = screen.getByRole("note", { name: "Sample data notice" });
    const main = screen.getByRole("main");
    const scroll = main.querySelector(":scope > .page__scroll");
    expect(note).toHaveTextContent("Nothing here was read from your machine, and nothing is running.");
    expect(scroll).toContainElement(screen.getByRole("heading", { name: "What should we work on?" }));
    expect(scroll).not.toContainElement(note);
    expect(note.parentElement).toBe(main);
    await user.click(within(note).getByRole("button", { name: "Hide sample data" }));
    expect(screen.queryByRole("note", { name: "Sample data notice" })).not.toBeInTheDocument();
    expect(screen.queryByText("Move auth to server-side sessions")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Set up Synapse" })).toBeInTheDocument();
  });

  it("shows no sessions, and nothing running, once sample data is off", async () => {
    const user = userEvent.setup();
    renderApp(false);
    await user.click(rail().getByRole("link", { name: "Sessions" }));
    expect(await screen.findByRole("heading", { name: "No sessions yet" })).toBeInTheDocument();
    expect(screen.queryByText(/running/i)).not.toBeInTheDocument();
  });

  it("has no sample banner on pages that carry no sample data", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(rail().getByRole("link", { name: "Agents" }));
    await screen.findByRole("heading", { level: 1, name: "Agents" });
    expect(screen.queryByRole("note", { name: "Sample data notice" })).not.toBeInTheDocument();
    expect(rail().getByRole("button", { name: "Waiting on you" }).querySelector("b")).toBeNull();
  });

  it("never marks an agent capability as verified", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(rail().getByRole("link", { name: "Agents" }));
    await screen.findByRole("heading", { level: 1, name: "Agents" });
    const tables = screen.getAllByRole("table");
    for (const t of tables) expect(t.querySelector(".ev--ver, .ev--adv")).toBeNull();
    expect(screen.getByRole("note")).toHaveTextContent("Nothing has been checked.");
    // Setup facts stay unknown for kimi; only the adapter row and capabilities say there is no adapter.
    expect(within(tables[0]).getByText("none")).toBeInTheDocument();
    expect(within(tables[0]).queryByText("no adapter")).not.toBeInTheDocument();
    expect(within(tables[1]).getAllByText("no adapter").length).toBeGreaterThan(0);
  });
});

describe("controls", () => {
  it("keeps unavailable controls focusable and prevents activation", async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    render(
      <Button reason="This action is unavailable." onClick={onClick}>
        Unavailable action
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Unavailable action" });

    button.focus();
    expect(button).toHaveFocus();
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(button).toHaveAccessibleDescription("This action is unavailable.");

    await user.click(button);
    button.focus();
    await user.keyboard("{Enter} ");
    expect(onClick).not.toHaveBeenCalled();
  });

  it("disables execution-shaped actions and states why", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(rail().getByRole("link", { name: "Projects" }));
    const add = await screen.findByRole("button", { name: "Add a folder…" });
    expect(add).toHaveAttribute("aria-disabled", "true");
    expect(add).toHaveAccessibleDescription(/never reads your filesystem/i);
    expect(screen.getByRole("button", { name: "Draft task here" })).not.toHaveAttribute("aria-disabled");
  });

  it("every disabled button on every page has a stated reason", async () => {
    const user = userEvent.setup();
    renderApp();
    for (const name of ["Home", "Projects", "Sessions", "Context", "Agents", "Settings"]) {
      await user.click(rail().getByRole("link", { name }));
      await screen.findByRole("heading", { level: 1, name: name === "Home" ? "What should we work on?" : name });
      for (const b of screen.getAllByRole("button").filter((el) => el.getAttribute("aria-disabled") === "true")) {
        expect(b, `${name}: "${b.textContent}"`).toHaveAccessibleDescription(/.+/);
      }
    }
  });

  it("selecting a project updates the detail pane", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(rail().getByRole("link", { name: "Projects" }));
    await user.click(await screen.findByRole("button", { name: /mochi-web/ }));
    expect(screen.getByRole("heading", { level: 2, name: "mochi-web" })).toBeInTheDocument();
    expect(screen.getByText("No vault for this project")).toBeInTheDocument();
  });

  it("settings tabs switch with the keyboard", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(rail().getByRole("link", { name: "Settings" }));
    const prefs = await screen.findByRole("tab", { name: "Preferences" });
    prefs.focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Limits" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Limits" })).toHaveFocus();
  });
});
