import { useState } from "react";
import { hrefFor } from "../app/routes";
import { useSampleData } from "../app/SampleData";
import { useWorkspace } from "../app/Workspace";
import { MOCK_PROJECTS } from "../fixtures/mock";
import { filterSessions, STATE_LABEL, STATE_TONE, STATES, usableSessions, type SessionFilter } from "../fixtures/sessions";
import { Continuity } from "../components/Continuity";
import { Agent, Button, Empty, PageTitle, Select } from "../components/ui";

const NONE: SessionFilter = { state: "all", project: "all", query: "" };

export function Sessions() {
  const { on, setOn } = useSampleData();
  const { session, selectSession } = useWorkspace();
  const [filter, setFilter] = useState<SessionFilter>(NONE);

  const all = on ? usableSessions() : [];
  const shown = filterSessions(all, filter);
  const filtered = filter.state !== "all" || filter.project !== "all" || filter.query.trim() !== "";
  // A selection the filters hide is not shown as selected, and has no comparison below.
  const cur = shown.find((s) => s.id === session);
  const tally = STATES.filter((st) => all.some((s) => s.state === st))
    .map((st) => `${all.filter((s) => s.state === st).length} ${st}`)
    .join(" · ");

  return (
    <div className="col col--1300">
      <div className="flex flex-wrap items-end gap-5">
        <PageTitle lede={`Every agent run, across projects. Terminals for a single task live inside that task.${on ? " These are synthetic examples: none was ever started." : ""}`}>
          Sessions
        </PageTitle>
        <span className="sp" />
        <span className="meta">{on ? `sample · ${tally}` : "0 sessions"}</span>
      </div>

      {!on ? (
        <Empty
          title="No sessions yet"
          why="sessions: 0 · no agent has been started by this app"
          actions={<Button onClick={() => setOn(true)}>Show sample data</Button>}
        >
          A session is one agent CLI working in one worktree. None can start from this build: it doesn’t run agents or contact
          providers.
        </Empty>
      ) : (
        <>
          <section aria-label="Sample sessions">
            <div className="sfilter" role="search" aria-label="Filter sample sessions">
              <div className="search">
                <label className="sr" htmlFor="sq">Search sessions by task, agent or id</label>
                <input
                  id="sq"
                  type="search"
                  value={filter.query}
                  placeholder="Search task, agent or id, e.g. codex"
                  onChange={(e) => setFilter({ ...filter, query: e.target.value })}
                />
              </div>
              <label className="t-label" htmlFor="sf-state">State</label>
              <Select id="sf-state" value={filter.state} onChange={(e) => setFilter({ ...filter, state: e.target.value as SessionFilter["state"] })}>
                <option value="all">All states</option>
                {STATES.map((st) => (
                  <option key={st} value={st}>{STATE_LABEL[st]}</option>
                ))}
              </Select>
              <label className="t-label" htmlFor="sf-project">Project</label>
              <Select id="sf-project" value={filter.project} onChange={(e) => setFilter({ ...filter, project: e.target.value })}>
                <option value="all">All projects</option>
                {MOCK_PROJECTS.map((p) => (
                  <option key={p.name}>{p.name}</option>
                ))}
              </Select>
              {filtered && (
                <Button small variant="ghost" onClick={() => setFilter(NONE)}>
                  Clear filters
                </Button>
              )}
            </div>
            <p className="mono meta mb-2" aria-live="polite">
              {shown.length} of {all.length} sample sessions
            </p>

            {shown.length === 0 ? (
              <Empty
                small
                title="No sessions match"
                why={`${all.length} sample sessions exist · filters match 0`}
                actions={<Button onClick={() => setFilter(NONE)}>Clear filters</Button>}
              >
                Try a different state or project, or fewer search words.
              </Empty>
            ) : (
              <>
                <div className="hdr t-label sli-hdr" aria-hidden="true">
                  <span>Agent</span>
                  <span>Task</span>
                  <span>Project</span>
                  <span>State</span>
                  <span>Worktree</span>
                </div>
                <ul className="slist" aria-label="Sample sessions">
                  {shown.map((s) => (
                    <li key={s.id} className="sli">
                      <button
                        type="button"
                        className={`srow${s.id === cur?.id ? " on" : ""}`}
                        aria-pressed={s.id === cur?.id}
                        onClick={() => selectSession(s.id)}
                      >
                        <Agent id={s.agent}>{s.agentName}</Agent>
                        <span className="min-w-0">
                          {s.task} <span className="sr">, session {s.id}</span>
                        </span>
                        <span className="mono meta">{s.project}</span>
                        <span className={`st st--${STATE_TONE[s.state]}`}>
                          {STATE_LABEL[s.state].toLowerCase()}{s.tag ? ` · ${s.tag}` : ""}
                        </span>
                        <span className="mono meta">{s.worktree ?? "none"}</span>
                      </button>
                      <a className="btn btn--sm btn--ghost sli__open" href={hrefFor("sessions", s.id)} aria-label={`Open details for session ${s.id}, ${s.task}`}>
                        Details
                      </a>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </section>

          {cur ? (
            <Continuity session={cur} />
          ) : (
            shown.length > 0 && (
              <Empty small title="No session selected" why="select a row to compare ways to pick it up">
                Choose a sample session above to see how continuing, starting a new run and repeating differ.
              </Empty>
            )
          )}
        </>
      )}
    </div>
  );
}
