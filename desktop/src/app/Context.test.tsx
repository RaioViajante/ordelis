import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { countText, excerptOf, EXCERPT_LIMIT_CHARS } from "./contextBudget";
import { SampleDataProvider } from "./SampleData";
import { Shell } from "./Shell";
import { MOCK_CONTEXT_ENTRIES, usableEntries, type ContextEntry } from "../fixtures/mock";

function renderApp(sample = true) {
  window.history.replaceState(null, "", "#/home");
  return render(
    <SampleDataProvider initial={sample}>
      <Shell />
    </SampleDataProvider>,
  );
}

const rail = () => within(screen.getByRole("navigation", { name: "Main" }));
const status = () => screen.getByRole("status");
const pick = (title: string) => screen.getByRole("checkbox", { name: `Select ${title}` });

async function openContext(user: ReturnType<typeof userEvent.setup>) {
  await user.click(rail().getByRole("link", { name: "Context" }));
  await screen.findByRole("heading", { level: 1, name: "Context" });
}

describe("Context sources and provenance", () => {
  it("shows three distinct sample sources and never claims a real vault", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    expect(screen.getByText("No Obsidian vault is connected. These notes are invented and never read from disk.")).toBeInTheDocument();
    for (const label of ["Sample notes", "Sample private history", "Sample pending suggestions"]) {
      expect(screen.getByRole("heading", { level: 3, name: new RegExp(label) })).toBeInTheDocument();
    }
    expect(screen.getByText("none connected")).toBeInTheDocument();
    expect(screen.queryByText(/1,284|Cérebro|~\/Obsidian|\.local\/share/)).not.toBeInTheDocument();
  });

  it("states provenance for every source kind in its excerpt", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    for (const title of ["Security / Tokens", "Decision · Oct 12", "Security / Rotation"]) {
      await user.click(screen.getByRole("button", { name: `Excerpt of ${title}` }));
      const region = screen.getByRole("region", { name: `Excerpt of ${title}, sample` });
      expect(within(region).getByText(/Synthetic sample .*invented for this UI/)).toBeInTheDocument();
    }
  });

  it("gives every fixture entry explicit provenance", () => {
    for (const e of MOCK_CONTEXT_ENTRIES) expect(e.provenance, e.id).toMatch(/Synthetic sample/);
  });
});

describe("Context search", () => {
  it("narrows results by every term and reports a no-results state", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    expect(screen.getByText("8 of 8 sample entries · atlas-api")).toBeInTheDocument();
    const search = screen.getByRole("searchbox", { name: "Search sample context" });
    await user.type(search, "revoked family");
    expect(screen.getByText("1 of 8 sample entries · atlas-api")).toBeInTheDocument();
    expect(pick("Security / Tokens")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "Select Journal / October 2" })).not.toBeInTheDocument();

    await user.clear(search);
    await user.type(search, "zzzz");
    expect(screen.getByRole("heading", { name: "No results" })).toBeInTheDocument();
    expect(screen.getByText("“zzzz” matched 0 of 8 sample entries")).toBeInTheDocument();
  });

  it("scopes results to the project and explains projects with no sample context", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    await user.selectOptions(screen.getByRole("combobox", { name: "Project" }), "mochi-web");
    expect(screen.getByText("1 of 1 sample entries · mochi-web")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "Select Security / Tokens" })).not.toBeInTheDocument();
    await user.selectOptions(screen.getByRole("combobox", { name: "Project" }), "synapse");
    expect(screen.getByRole("heading", { name: "No sample context for synapse" })).toBeInTheDocument();
    await user.selectOptions(screen.getByRole("combobox", { name: "Project" }), "");
    expect(screen.getByRole("heading", { name: "No project selected" })).toBeInTheDocument();
  });
});

describe("Context selection", () => {
  it("selects and removes entries with an announcement and visible status", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    expect(screen.getByText("No sample context selected")).toBeInTheDocument();
    await user.click(pick("Security / Tokens"));
    expect(pick("Security / Tokens")).toBeChecked();
    expect(screen.getByText("1 sample entry selected")).toBeInTheDocument();
    expect(status()).toHaveTextContent("Security / Tokens added to sample context");
    await user.click(pick("Security / Tokens"));
    expect(screen.getByText("No sample context selected")).toBeInTheDocument();
    expect(status()).toHaveTextContent("Security / Tokens removed from sample context");

    await user.click(pick("Journal / October 2"));
    await user.click(pick("Decision · Oct 12"));
    await user.click(screen.getByRole("button", { name: "Clear selection" }));
    expect(screen.getByText("No sample context selected")).toBeInTheDocument();
    expect(status()).toHaveTextContent("Sample context selection cleared");
  });

  it("does not let drafts or unavailable records become context", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    expect(screen.queryByRole("checkbox", { name: "Select Security / Rotation" })).not.toBeInTheDocument();
    expect(screen.getAllByText(/Not selectable: A draft is not context until it is accepted/)).toHaveLength(2);
    expect(screen.queryByRole("checkbox", { name: "Select Runbooks / Old deploys" })).not.toBeInTheDocument();
    expect(screen.getByText("Not selectable: this sample record is marked unavailable.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Excerpt of Runbooks / Old deploys" })).not.toBeInTheDocument();
  });

  it("drops history selections when past decisions stop being offered, and does not resurrect them", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    await user.click(pick("Security / Tokens"));
    await user.click(pick("Decision · Oct 12"));
    expect(screen.getByText("2 sample entries selected")).toBeInTheDocument();
    const toggle = screen.getByRole("checkbox", { name: "Offer past decisions as context" });
    await user.click(toggle);
    expect(screen.getByText("1 sample entry selected")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "Select Decision · Oct 12" })).not.toBeInTheDocument();
    await user.click(toggle);
    expect(pick("Decision · Oct 12")).not.toBeChecked();
    expect(screen.getByText("1 sample entry selected")).toBeInTheDocument();
  });
});

describe("Home, Context and Preview stay in sync", () => {
  it("carries a Home selection into Context and a Context selection back to Home", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(screen.getByRole("button", { name: "Select context" }));
    await user.click(screen.getByRole("checkbox", { name: /Security \/ Tokens/ }));
    await openContext(user);
    expect(pick("Security / Tokens")).toBeChecked();
    await user.click(pick("Journal / October 2"));
    await user.click(rail().getByRole("link", { name: "Home" }));
    expect(screen.getByText("2 of 5 selected")).toBeInTheDocument();
  });

  it("shows the selection in the draft preview and says previewing authorizes nothing", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.type(screen.getByRole("textbox", { name: "Describe the task" }), "move auth");
    await openContext(user);
    await user.click(pick("Security / Tokens"));
    await user.click(screen.getByRole("button", { name: "Preview task" }));
    await screen.findByRole("heading", { level: 1, name: "Task preview" });
    const list = screen.getByRole("list", { name: "Selected sample context" });
    expect(within(list).getByText("Security / Tokens")).toBeInTheDocument();
    expect(screen.getByText(/does not dispatch a task and does not authorize access to a vault or a provider/)).toBeInTheDocument();
    expect(screen.getByText(/estimated, not measured/)).toBeInTheDocument();

    await user.click(screen.getByRole("link", { name: "Change in Context" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Context" })).toBeInTheDocument();
    expect(pick("Security / Tokens")).toBeChecked();
  });

  it("disables Preview task in Context until there is a draft, with the reason", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    const preview = screen.getByRole("button", { name: "Preview task" });
    expect(preview).toHaveAttribute("aria-disabled", "true");
    expect(preview).toHaveAccessibleDescription("Describe the task on Home first.");
  });

  it("clears the selection when the project changes, and not just in the view", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    await user.click(pick("Security / Tokens"));
    await user.selectOptions(screen.getByRole("combobox", { name: "Project" }), "mochi-web");
    expect(screen.getByText("No sample context selected")).toBeInTheDocument();
    await user.selectOptions(screen.getByRole("combobox", { name: "Project" }), "atlas-api");
    expect(pick("Security / Tokens")).not.toBeChecked();
  });
});

describe("sample mode off", () => {
  it("hides the sample context, offers a way back and keeps the draft", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.type(screen.getByRole("textbox", { name: "Describe the task" }), "keep me");
    await openContext(user);
    await user.click(pick("Security / Tokens"));
    await user.click(screen.getByRole("button", { name: "Hide sample data" }));
    expect(screen.getByRole("heading", { name: "Sample context is hidden" })).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: /^Select / })).not.toBeInTheDocument();
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Show sample data" }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Project" }), "atlas-api");
    expect(pick("Security / Tokens")).not.toBeChecked();
    await user.click(rail().getByRole("link", { name: "Home" }));
    expect(screen.getByRole("textbox", { name: "Describe the task" })).toHaveValue("keep me");
  });

  it("is empty when sample data starts hidden", async () => {
    const user = userEvent.setup();
    renderApp(false);
    await openContext(user);
    expect(screen.getByRole("heading", { name: "Sample context is hidden" })).toBeInTheDocument();
    expect(screen.getByText("0 notes · sample data hidden")).toBeInTheDocument();
  });
});

describe("invalid sample records", () => {
  const good: ContextEntry = {
    id: "a", source: "notes", project: "atlas-api", title: "T", where: "W", text: "body", provenance: "Synthetic sample",
  };
  it("drops malformed records and duplicate ids", () => {
    const bad = [
      { ...good, id: "" },
      { ...good, id: "b", text: "   " },
      { ...good, id: "c", source: "web" as never },
      { ...good, id: "d", project: "ghost" },
      { ...good, id: "e", provenance: "" },
      { ...good, id: "a" },
    ];
    expect(usableEntries([good, ...bad]).map((e) => e.id)).toEqual(["a"]);
  });
});

describe("budget semantics", () => {
  it("counts characters, UTF-8 bytes and estimated tokens deterministically", () => {
    expect(countText("abcd")).toEqual({ chars: 4, bytes: 4, estTokens: 1 });
    expect(countText("é")).toEqual({ chars: 1, bytes: 2, estTokens: 1 });
    expect(countText("😀")).toEqual({ chars: 1, bytes: 4, estTokens: 1 });
    expect(countText("abcde").estTokens).toBe(2);
    expect(countText("")).toEqual({ chars: 0, bytes: 0, estTokens: 0 });
  });

  it("cuts long excerpts at the limit and reports what was omitted", () => {
    const full = "x".repeat(EXCERPT_LIMIT_CHARS + 30);
    const ex = excerptOf(full);
    expect(ex.shown).toHaveLength(EXCERPT_LIMIT_CHARS);
    expect(ex.omittedChars).toBe(30);
    expect(excerptOf("short")).toEqual({ shown: "short", omittedChars: 0 });
  });

  it("labels values as estimated and illustrative, and shows the cut", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    await user.click(pick("Security / Tokens"));
    const tokens = MOCK_CONTEXT_ENTRIES.find((e) => e.id === "tokens")!;
    const ex = excerptOf(tokens.text);
    const counts = countText(ex.shown);
    expect(ex.omittedChars).toBeGreaterThan(0);
    expect(screen.getByText(`~${counts.estTokens} of 6,000 example`)).toBeInTheDocument();
    expect(screen.getByText(`${counts.bytes} UTF-8`)).toBeInTheDocument();
    expect(screen.getByText(/Nothing was measured by a provider, and the budget is an illustration, not an enforced limit/)).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: /Estimated tokens against the 6,000 token example budget/ })).toHaveValue(counts.estTokens);

    await user.click(screen.getByRole("button", { name: "Excerpt of Security / Tokens" }));
    expect(screen.getByText(new RegExp(`cut here · ${ex.omittedChars} chars not shown`))).toBeInTheDocument();
    expect(screen.getAllByText(/est\. tok/).length).toBeGreaterThan(0);
    expect(screen.queryByText(/\bmeasured usage\b/i)).not.toBeInTheDocument();
  });
});

describe("keyboard and assistive technology", () => {
  it("toggles an excerpt from the keyboard and exposes its state", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    const button = screen.getByRole("button", { name: "Excerpt of Security / Tokens" });
    expect(button).toHaveAttribute("aria-expanded", "false");
    button.focus();
    await user.keyboard("{Enter}");
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(button).toHaveFocus();
    expect(document.getElementById(button.getAttribute("aria-controls")!)).toBeInTheDocument();
    await user.keyboard(" ");
    expect(button).toHaveAttribute("aria-expanded", "false");
  });

  it("selects with the space bar and keeps focus on the checkbox", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    pick("Journal / October 2").focus();
    await user.keyboard(" ");
    expect(pick("Journal / October 2")).toBeChecked();
    expect(pick("Journal / October 2")).toHaveFocus();
    expect(status()).toHaveTextContent("Journal / October 2 added to sample context");
  });

  it("keeps one status region, labels results as sample and announces result counts politely", async () => {
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    expect(screen.getAllByRole("status")).toHaveLength(1);
    expect(screen.getByText("8 of 8 sample entries · atlas-api")).toHaveAttribute("aria-live", "polite");
    expect(screen.getByRole("searchbox", { name: "Search sample context" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Selection and sample source" })).toBeInTheDocument();
  });
});

describe("layout and boundaries", () => {
  const css = readFileSync(join(__dirname, "..", "styles", "index.css"), "utf8");

  it("defines wrapping and collapsing rules for the 760 px minimum window", () => {
    expect(css).toMatch(/\.ctx-row__head\s*{[^}]*flex-wrap:\s*wrap/);
    expect(css).toMatch(/\.ctx-row__title\s*{[^}]*overflow-wrap:\s*anywhere/);
    expect(css).toMatch(/@media \(max-width: 1100px\)\s*{\s*\.split/);
    expect(css).toMatch(/@media \(max-width: 1000px\)[\s\S]*?\.src3/);
  });

  it("persists nothing while selecting", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const user = userEvent.setup();
    renderApp();
    await openContext(user);
    await user.click(pick("Security / Tokens"));
    await user.type(screen.getByRole("searchbox"), "token");
    expect(setItem).not.toHaveBeenCalled();
    setItem.mockRestore();
    expect(document.cookie).toBe("");
  });
});
