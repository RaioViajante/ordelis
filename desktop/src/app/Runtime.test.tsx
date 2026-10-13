import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StrictMode } from "react";
import { Settings } from "../pages/Settings";
import { SampleDataProvider } from "./SampleData";

// Mocks exist only here: production code has no fallback answer of any kind.
const valid = () => ({
  schema_version: 1,
  provenance: { source: "python_runtime", interpreter_kind: "dev_checkout", observed_at_ms: 1_776_000_000_000, duration_ms: 84 },
  runtime: { version: "0.12.1", python: "3.14.7" },
  gate: { policy_version: 1 },
});
const failure = (code: string) => ({ code, message: "Fixed native message that must not be shown." });

const flag = globalThis as { isTauri?: boolean };

// Every native answer is held until the test releases it, so loading states can be observed.
let calls: string[] = [];
let pending: { resolve: (v: unknown) => void; reject: (e: unknown) => void }[] = [];
function desktop() {
  flag.isTauri = true;
  mockIPC((cmd) => {
    calls.push(cmd);
    return new Promise((resolve, reject) => pending.push({ resolve, reject }));
  });
}
async function answer(fn: (p: (typeof pending)[number]) => void) {
  const next = pending.shift();
  if (!next) throw new Error("no request is pending");
  await act(async () => fn(next));
}

beforeEach(() => {
  calls = [];
  pending = [];
});
afterEach(async () => {
  // Settle anything left so the adapter's shared in-flight request never leaks into the next test.
  for (const p of pending.splice(0)) await act(async () => p.resolve(valid()));
  delete flag.isTauri;
  clearMocks();
});

async function openRuntime(strict = false) {
  const user = userEvent.setup();
  const tree = (
    <SampleDataProvider>
      <Settings />
    </SampleDataProvider>
  );
  const view = render(strict ? <StrictMode>{tree}</StrictMode> : tree);
  await user.click(screen.getByRole("tab", { name: "Runtime" }));
  return { user, view };
}
const section = () => screen.getByRole("region", { name: "Runtime" });
const state = () => screen.getByRole("status");
const refresh = () => within(section()).getByRole("button", { name: "Refresh" });

describe("runtime status in Settings", () => {
  it("does not query anything in a browser preview and says so", async () => {
    mockIPC((cmd) => calls.push(cmd));
    const { user } = await openRuntime();
    expect(state()).toHaveTextContent("Browser preview");
    expect(section()).toHaveTextContent("no connection to the desktop app, so no runtime was queried");
    expect(refresh()).toHaveAttribute("aria-disabled", "true");
    expect(refresh()).toHaveAccessibleDescription("Runtime status can only be read inside the desktop app.");
    await user.click(refresh());
    expect(calls).toEqual([]);
    // Sample data is on, yet nothing resembling runtime identity is invented.
    expect(within(section()).queryByRole("term")).toBeNull();
    expect(section()).not.toHaveTextContent(/\d+\.\d+\.\d+/);
  });

  it("queries once on opening the tab, showing a quiet loading state", async () => {
    desktop();
    await openRuntime();
    expect(calls).toEqual(["get_runtime_status"]);
    expect(state()).toHaveTextContent("Checking the runtime…");
    expect(section()).toHaveAttribute("aria-busy", "true");
    expect(refresh()).toHaveAttribute("aria-disabled", "true");
  });

  it("shows the validated identity with labelled fields", async () => {
    desktop();
    await openRuntime();
    await answer((p) => p.resolve(valid()));
    expect(state()).toHaveTextContent("Answered by the runtime");
    const fields = Object.fromEntries(
      within(section())
        .getAllByRole("term")
        .map((dt) => [dt.textContent, dt.nextElementSibling?.textContent]),
    );
    expect(fields["Runtime version"]).toBe("0.12.1");
    expect(fields["Python"]).toBe("3.14.7");
    expect(fields["Gate policy"]).toMatch(/^version 1.*Metadata only: it authorizes nothing\.$/);
    expect(fields["Source"]).toBe("Python runtime · development checkout");
    expect(fields["Observed"]).toMatch(/· answered in 84 ms$/);
    expect(section()).toHaveAttribute("aria-busy", "false");
    expect(refresh()).not.toHaveAttribute("aria-disabled", "true");
  });

  it.each([
    ["runtime_not_installed", "Not available in this build", "Release builds never include the development runtime."],
    ["unsupported_platform", "Not supported on this platform", "runs on macOS only"],
    ["runtime_timeout", "The runtime didn’t answer in time", "stopped after 5 seconds"],
    ["runtime_output_malformed", "The runtime’s answer wasn’t understood", "discarded"],
    ["runtime_busy", "Another query is running", "Refresh again in a moment."],
    ["runtime_crashed", "Status couldn’t be read", "nothing is shown"],
  ])("shows fixed copy for %s and no native text", async (code, title, detail) => {
    desktop();
    await openRuntime();
    await answer((p) => p.reject(failure(code)));
    expect(state()).toHaveTextContent(title);
    expect(section()).toHaveTextContent(detail);
    expect(section()).not.toHaveTextContent("Fixed native message");
    expect(within(section()).queryByRole("term")).toBeNull();
  });

  it("treats a response outside the contract as unreadable and renders none of it", async () => {
    desktop();
    await openRuntime();
    await answer((p) => p.resolve({ ...valid(), interpreter: "/Users/someone/bin/python3" }));
    expect(state()).toHaveTextContent("Status couldn’t be read");
    expect(section()).not.toHaveTextContent("/Users");
    expect(section()).not.toHaveTextContent("0.12.1");
  });

  it("refreshes on request, keeps the last answer visible, and ignores clicks while one is running", async () => {
    desktop();
    const { user } = await openRuntime();
    await answer((p) => p.resolve(valid()));
    await user.click(refresh());
    await user.click(refresh());
    await user.click(refresh());
    expect(calls).toHaveLength(2);
    expect(state()).toHaveTextContent("Checking the runtime…");
    expect(section()).toHaveTextContent("0.12.1");
    await answer((p) => p.reject(failure("runtime_busy")));
    expect(state()).toHaveTextContent("Another query is running");
    // Busy is recoverable: Refresh works again and nothing retried by itself.
    expect(calls).toHaveLength(2);
    await user.click(refresh());
    await answer((p) => p.resolve({ ...valid(), runtime: { version: "0.12.2", python: "3.14.7" } }));
    expect(calls).toHaveLength(3);
    expect(section()).toHaveTextContent("0.12.2");
  });

  it("hides the previous error's detail while a refresh is running", async () => {
    desktop();
    const { user } = await openRuntime();
    await answer((p) => p.reject(failure("runtime_timeout")));
    expect(section()).toHaveTextContent("stopped after 5 seconds");
    await user.click(refresh());
    expect(state()).toHaveTextContent("Checking the runtime…");
    expect(section()).not.toHaveTextContent("stopped after 5 seconds");
    await answer((p) => p.reject(failure("runtime_failed")));
    expect(state()).toHaveTextContent("The runtime query failed");
    expect(section()).toHaveTextContent("The runtime exited with an error.");
  });

  it("never polls or retries on its own", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      desktop();
      await openRuntime();
      await answer((p) => p.reject(failure("runtime_timeout")));
      await act(async () => vi.advanceTimersByTime(10 * 60_000));
      expect(calls).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
  });

  // These check what a user can observe. They do not prove the hook's mounted guard: React 19 silently ignores
  // a state update on an unmounted component, so the tests pass with or without it.
  it("shares the in-flight request when the tab is left and reopened", async () => {
    desktop();
    const { user } = await openRuntime();
    await user.click(screen.getByRole("tab", { name: "Preferences" }));
    await user.click(screen.getByRole("tab", { name: "Runtime" }));
    expect(calls).toHaveLength(1);
    await answer((p) => p.resolve(valid()));
    expect(state()).toHaveTextContent("Answered by the runtime");
  });

  it("does not carry an answer that arrived while unmounted into the next mount", async () => {
    desktop();
    const { view } = await openRuntime();
    view.unmount();
    await answer((p) => p.reject(failure("runtime_failed")));
    await openRuntime();
    expect(calls).toHaveLength(2);
    expect(state()).toHaveTextContent("Checking the runtime…");
    expect(section()).not.toHaveTextContent("The runtime exited with an error.");
    await answer((p) => p.resolve(valid()));
    expect(state()).toHaveTextContent("Answered by the runtime");
  });

  it("sends a single request under React Strict Mode", async () => {
    desktop();
    await openRuntime(true);
    expect(calls).toEqual(["get_runtime_status"]);
    await answer((p) => p.resolve(valid()));
    expect(state()).toHaveTextContent("Answered by the runtime");
    expect(calls).toHaveLength(1);
  });

  it("reaches Refresh from the keyboard and keeps focus on it", async () => {
    desktop();
    const { user } = await openRuntime();
    await answer((p) => p.resolve(valid()));
    screen.getByRole("tab", { name: "Runtime" }).focus();
    await user.tab();
    expect(refresh()).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(calls).toHaveLength(2);
    expect(refresh()).toHaveFocus();
    await answer((p) => p.resolve(valid()));
    await user.keyboard(" ");
    expect(calls).toHaveLength(3);
  });

  it("announces outcomes through a polite status region and marks them with a symbol, not only colour", async () => {
    desktop();
    await openRuntime();
    await answer((p) => p.reject(failure("runtime_failed")));
    const status = state();
    expect(status).toHaveAttribute("aria-atomic", "true");
    expect(status.querySelector(".ev--bad")).toHaveTextContent("The runtime query failed");
  });
});
