import { useRef, useState, type KeyboardEvent } from "react";
import { useSampleData } from "../app/SampleData";
import { ROUTES } from "../app/routes";
import type { RuntimeErrorCode } from "../app/runtimeStatus";
import { useRuntimeStatus } from "../app/useRuntimeStatus";
import { Button, Empty, Evidence, PageTitle, SecHead, Vision, type EvidenceLevel } from "../components/ui";

const TABS = [
  { id: "preferences", label: "Preferences" },
  { id: "limits", label: "Limits" },
  { id: "notifications", label: "Notifications" },
  { id: "security", label: "Security policies" },
  { id: "runtime", label: "Runtime" },
] as const;
type TabId = (typeof TABS)[number]["id"];

export function Settings() {
  const [tab, setTab] = useState<TabId>("preferences");
  const refs = useRef<Record<string, HTMLButtonElement | null>>({});

  const onKey = (e: KeyboardEvent, i: number) => {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const next = TABS[(i + step + TABS.length) % TABS.length].id;
    setTab(next);
    refs.current[next]?.focus();
  };

  return (
    <div className="col col--1100">
      <PageTitle>Settings</PageTitle>
      <div className="tabs" role="tablist" aria-label="Settings sections">
        {TABS.map((t, i) => (
          <button
            key={t.id}
            ref={(el) => {
              refs.current[t.id] = el;
            }}
            type="button"
            role="tab"
            id={`tab-${t.id}`}
            aria-selected={tab === t.id}
            aria-controls="settings-panel"
            tabIndex={tab === t.id ? 0 : -1}
            onClick={() => setTab(t.id)}
            onKeyDown={(e) => onKey(e, i)}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div id="settings-panel" role="tabpanel" aria-labelledby={`tab-${tab}`} className="panel panel--wide">
        {tab === "preferences" && <Preferences />}
        {tab === "limits" && (
          <Empty title="No limits to set yet" why="limits: tokens · time · parallel sessions · planned">
            Limits are copied into each plan when it’s created. There are no plans or runs in this build, so there is nothing
            for a limit to govern.
          </Empty>
        )}
        {tab === "notifications" && (
          <Empty title="No notifications yet" why="delivery: in-app badge · system notifications planned">
            Notifications announce approvals, questions and blocked runs. Those only exist once agents run, and this build
            doesn’t run them.
          </Empty>
        )}
        {tab === "security" && <Security />}
        {tab === "runtime" && <Runtime />}
      </div>
    </div>
  );
}

function Preferences() {
  const { on, setOn } = useSampleData();
  return (
    <>
      <section>
        <SecHead title="Display" />
        <div className="rows">
          <div className="r">
            <span className="flex-1 text-[15px]">
              Theme <span className="meta block">Warm night is the only theme in this build</span>
            </span>
            <span className="lock">warm night</span>
          </div>
          <label className="set">
            <span>
              <span>Show sample data</span>
              <span className="meta">Invented fixtures for layout review. Kept in memory only and always labelled.</span>
            </span>
            <input
              className="switch"
              type="checkbox"
              checked={on}
              onChange={(e) => setOn(e.target.checked)}
              aria-label="Show sample data"
            />
          </label>
        </div>
      </section>
      <section>
        <SecHead title="Keyboard" meta="shortcuts work anywhere in the window" />
        <div className="rows">
          {ROUTES.map((r, i) => (
            <div className="r" key={r.id}>
              <span className="flex-1 text-[15px]">Go to {r.label}</span>
              <span className="lock">⌘{i + 1} · Ctrl+{i + 1}</span>
            </div>
          ))}
        </div>
      </section>
    </>
  );
}

const RULES: { action: string; note?: string; scope: string }[] = [
  { action: "Start an agent session", note: "no agent is ever launched from this app", scope: "—" },
  { action: "Run scripts or shell commands", note: "no command API exists", scope: "—" },
  { action: "Read or write project files", note: "the interface has no filesystem access", scope: "—" },
  { action: "Network access", note: "no remote content, fonts or analytics", scope: "—" },
  { action: "git push · publish · deploy", scope: "—" },
  { action: "Write to the Obsidian vault", scope: "— · not in this version" },
];

function Security() {
  return (
    <>
      <div className="notice notice--quiet">
        <span className="mono ink2">§</span>
        <div className="flex flex-col gap-1">
          <b className="font-medium">This build can’t act</b>
          <span className="ink2 text-[15px]">
            Every sensitive permission is simply unavailable, so there is nothing to approve. Editable policies, with a scope for
            each rule, arrive together with execution.
          </span>
        </div>
      </div>

      <section>
        <SecHead title="Rules" meta="enforced by what this build can’t do, not by a setting" />
        <div className="hdr3 t-label">
          <span>Action</span>
          <span>Decision</span>
          <span>Widest scope allowed</span>
        </div>
        {RULES.map((r) => (
          <div className="rule" key={r.action}>
            <span>
              {r.action}
              {r.note && <span className="meta">{r.note}</span>}
            </span>
            <span className="lock">never</span>
            <span className="lock">{r.scope}</span>
          </div>
        ))}
      </section>

      <section>
        <SecHead title="Authorized runs" meta="pinned to the policy they started with" />
        <div className="rows">
          <div className="r">
            <span className="flex-1 meta">No runs have been authorized.</span>
            <Vision>after execution ships</Vision>
          </div>
        </div>
      </section>
    </>
  );
}

// Fixed copy for every outcome. Nothing the native side sends is shown except the validated identity fields.
const OUTCOMES: Record<RuntimeErrorCode | "unreadable", { level: EvidenceLevel; title: string; detail: string }> = {
  runtime_not_installed: {
    level: "unk",
    title: "Not available in this build",
    detail:
      "Release builds never include the development runtime. A development build needs the dev-runtime feature and a configured interpreter.",
  },
  unsupported_platform: {
    level: "unk",
    title: "Not supported on this platform",
    detail: "The development runtime bridge runs on macOS only.",
  },
  runtime_busy: {
    level: "unk",
    title: "Another query is running",
    detail: "Only one runtime query runs at a time. Refresh again in a moment.",
  },
  runtime_unsupported_version: {
    level: "bad",
    title: "Unsupported runtime version",
    detail: "The configured Python or the runtime’s answer format isn’t supported. Python 3.10 or newer is required.",
  },
  runtime_timeout: { level: "bad", title: "The runtime didn’t answer in time", detail: "The query was stopped after 5 seconds." },
  runtime_output_too_large: {
    level: "bad",
    title: "The runtime’s answer was too large",
    detail: "It was discarded without being shown.",
  },
  runtime_output_malformed: {
    level: "bad",
    title: "The runtime’s answer wasn’t understood",
    detail: "It didn’t match the expected format and was discarded.",
  },
  runtime_failed: { level: "bad", title: "The runtime query failed", detail: "The runtime exited with an error." },
  unreadable: {
    level: "bad",
    title: "Status couldn’t be read",
    detail: "The desktop app answered with something this version doesn’t recognise, so nothing is shown.",
  },
};

const timeOf = (ms: number) =>
  new Date(ms).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });

function Runtime() {
  const { desktop, loading, result, refresh } = useRuntimeStatus();
  const outcome = result && result.kind !== "ok" ? OUTCOMES[result.kind === "error" ? result.code : "unreadable"] : null;
  const status = result?.kind === "ok" ? result.status : null;

  const state: { level: EvidenceLevel; title: string } = !desktop
    ? { level: "unk", title: "Browser preview" }
    : loading
      ? { level: "unk", title: "Checking the runtime…" }
      : status
        ? { level: "ver", title: "Answered by the runtime" }
        : (outcome ?? OUTCOMES.unreadable);

  return (
    <section aria-labelledby="runtime-h" aria-busy={loading}>
      <SecHead title={<span id="runtime-h">Runtime</span>} meta="local development environment">
        {desktop ? (
          <Button small aria-disabled={loading} onClick={refresh}>
            Refresh
          </Button>
        ) : (
          <Button small reason="Runtime status can only be read inside the desktop app.">
            Refresh
          </Button>
        )}
      </SecHead>
      <div className="runtime">
        <p role="status" aria-atomic="true">
          <Evidence level={state.level}>{state.title}</Evidence>
        </p>
        {!desktop && (
          <p className="ink2">
            This page is running in a browser, which has no connection to the desktop app, so no runtime was queried.
          </p>
        )}
        {outcome && !loading && <p className="ink2">{outcome.detail}</p>}
        {status && (
          <dl className="kv2">
            <dt>Runtime version</dt>
            <dd>{status.runtime.version}</dd>
            <dt>Python</dt>
            <dd>{status.runtime.python}</dd>
            <dt>Gate policy</dt>
            <dd>
              version {status.gate.policy_version}
              <span className="meta">Execution Safety Gate policy version. Metadata only: it authorizes nothing.</span>
            </dd>
            <dt>Source</dt>
            <dd>Python runtime · development checkout</dd>
            <dt>Observed</dt>
            <dd>
              {timeOf(status.provenance.observed_at_ms)} · answered in {status.provenance.duration_ms} ms
            </dd>
          </dl>
        )}
        <p className="meta">
          Read once when this tab opens and again only when you press Refresh. Other pages show sample data; this section
          never does. It doesn’t check agents, providers or models, and the runtime bridge is not a sandbox.
        </p>
      </div>
    </section>
  );
}
