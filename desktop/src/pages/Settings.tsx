import { useRef, useState, type KeyboardEvent } from "react";
import { useSampleData } from "../app/SampleData";
import { ROUTES } from "../app/routes";
import { Empty, PageTitle, SecHead, Vision } from "../components/ui";

const TABS = [
  { id: "preferences", label: "Preferences" },
  { id: "limits", label: "Limits" },
  { id: "notifications", label: "Notifications" },
  { id: "security", label: "Security policies" },
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
