import { useState } from "react";
import { useAddFolder, useRegistry } from "../app/Registry";
import { useSampleData } from "../app/SampleData";
import { hrefFor, type Location } from "../app/routes";
import { useWorkspace } from "../app/Workspace";
import { MOCK_PROJECTS, SOURCE_LABEL, MOCK_TASKS, MOCK_WAITING } from "../fixtures/mock";
import { Agent, Button, SecHead, Select } from "../components/ui";
import { StatusText } from "./Projects";

export function Home({ go }: { go: (page: Location["page"], param?: string) => void }) {
  const { on } = useSampleData();
  const { project, selectProject, draft, setDraft, clearDraft, selectable: suggestions, contextIds, toggleContext } = useWorkspace();
  const [picking, setPicking] = useState(false);
  const chosen = contextIds.length;
  const blocker = !on
    ? "Sample data is hidden. Drafting a task in your own folders isn't built yet."
    : !project
      ? "Choose a project first."
      : !draft.trim()
        ? "Describe the task first."
        : undefined;

  return (
    <div className="col col--home">
      <section className="flex max-w-[840px] flex-col gap-4">
        <h1 className="h1--home">What should we work on?</h1>
        <div className="ask">
          <label className="sr" htmlFor="new-task">
            Describe the task
          </label>
          <textarea
            id="new-task"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="Describe a change, a fix, or a question about the code…"
          />
          <div className="ask__bar">
            <label className="sr" htmlFor="proj">
              Project
            </label>
            <Select
              id="proj"
              className="field"
              value={on ? (project ?? "") : ""}
              disabled={!on}
              onChange={(e) => selectProject(e.target.value || null)}
            >
              {on ? (
                <>
                  <option value="">Choose a project…</option>
                  {MOCK_PROJECTS.map((p) => (
                    <option key={p.name}>{p.name}</option>
                  ))}
                </>
              ) : (
                <option value="">No sample projects</option>
              )}
            </Select>
            <span className="t-label">
              context:{" "}
              {on && suggestions.length > 0 ? (
                <>
                  <span className="prov prov--you">sample context</span>{" "}
                  <span className="meta">{chosen ? `${chosen} of ${suggestions.length} selected` : `${suggestions.length} available`}</span>
                </>
              ) : (
                <span className="meta">none selected</span>
              )}
            </span>
            <span className="ask__actions">
              {draft && (
                <Button small variant="ghost" onClick={clearDraft}>
                  Clear draft
                </Button>
              )}
              {on && suggestions.length > 0 ? (
                <Button aria-expanded={picking} aria-controls="ctx-pick" onClick={() => setPicking(!picking)}>
                  Select context
                </Button>
              ) : (
                <Button reason={on ? "This project has no sample context entries." : "No sample context while sample data is hidden."}>
                  Select context
                </Button>
              )}
              <Button variant="go" reason={blocker} onClick={() => go("preview")}>
                Preview task
              </Button>
            </span>
          </div>
          {picking && on && suggestions.length > 0 && (
            <fieldset className="ctx-pick" id="ctx-pick">
              <legend className="t-label">Sample context for {project} · nothing is read or sent</legend>
              {suggestions.map((n) => (
                <label key={n.id}>
                  <input type="checkbox" checked={contextIds.includes(n.id)} onChange={() => toggleContext(n.id)} />
                  <span>{n.title}</span>
                  <span className="meta">{SOURCE_LABEL[n.source]}</span>
                </label>
              ))}
              <a className="link meta" href={hrefFor("context")}>
                Search and read excerpts in Context
              </a>
            </fieldset>
          )}
        </div>
        <p className="hint">Preview only: nothing here starts an agent. Your draft stays while you move around the app.</p>
      </section>

      <RecentProjects />

      {on ? <SampleSections /> : <FirstUse />}
    </div>
  );
}

function SampleSections() {
  return (
    <>
      <section>
        <SecHead title="Waiting on you">
          <span className="ml-auto">
            <Button variant="link" reason="The inbox arrives with task planning; there is nothing to open yet.">
              Open inbox →
            </Button>
          </span>
        </SecHead>
        <div className="border-t border-line-100">
          {MOCK_WAITING.map((w) => (
            <div className="wait" key={w.text}>
              <span className={`kind k-${w.kind === "approval" ? "appr" : w.kind === "question" ? "q" : "blk"}`}>{w.kind}</span>
              <Agent id={w.agent}>{w.agentName}</Agent>
              <span className="flex-1">{w.mono ? <span className="mono">{w.text}</span> : w.text}</span>
              <span className="meta">{w.project}</span>
            </div>
          ))}
        </div>
      </section>

      <section>
        <SecHead title="Active tasks" meta="across projects · sample" />
        <div className="border-t border-line-100">
          {MOCK_TASKS.map((t) => (
            <a className="task" key={t.id} href={hrefFor("task", t.id)}>
              <span>{t.title}</span>
              <span className="mono meta">{t.project}</span>
              <span className="flex gap-3">
                {t.agents.map((a) => (
                  <Agent key={a.id} id={a.id}>
                    {a.name}
                  </Agent>
                ))}
              </span>
              <span className={`st st--${t.tone}`}>{t.state}</span>
            </a>
          ))}
        </div>
      </section>
    </>
  );
}

function Result({ message }: { message: ReturnType<typeof useAddFolder>["message"] }) {
  if (!message) return null;
  return (
    <div className={`notice ${message.tone === "ok" ? "notice--quiet" : ""}`} role={message.tone === "error" ? "alert" : "note"}>
      <p>{message.text}</p>
    </div>
  );
}

// Folders the user has added, newest first. Real data from the project registry; in a browser preview there is none.
function RecentProjects() {
  const { desktop, loading, projects, listError } = useRegistry();
  const { message, reason, add, adding, id } = useAddFolder();
  if (!desktop) return null;
  const recent = [...projects].sort((a, b) => b.registered_at_ms - a.registered_at_ms).slice(0, 5);
  return (
    <section aria-label="Recent projects">
      <SecHead title="Recent projects" meta="your folders">
        <span className="ml-auto">
          <Button id={id} small reason={reason} onClick={add}>
            {adding ? "Choose a folder…" : "Add a folder…"}
          </Button>
        </span>
      </SecHead>
      <Result message={message} />
      <div className="rows">
        {loading && <div className="r meta">Loading your folders…</div>}
        {!loading && recent.length === 0 && !listError && (
          <div className="r meta">No folders yet. Add one to see it here. Ordelis saves only its location.</div>
        )}
        {recent.map((p) => (
          <a className="r" key={p.id} href={hrefFor("projects", p.id)}>
            <span className="bidi min-w-0 flex-1 text-[15px]" dir="auto">
              {p.name}
            </span>
            <span className="bidi mono meta" dir="auto">
              {p.path_display}
            </span>
            <StatusText status={p.status} />
          </a>
        ))}
      </div>
      {projects.length > recent.length && (
        <a className="link meta" href={hrefFor("projects")}>
          All {projects.length} projects
        </a>
      )}
    </section>
  );
}

function FirstUse() {
  const { projects } = useRegistry();
  const { reason, add, adding, id } = useAddFolder();
  return (
    <section className="empty max-w-[840px]">
      <h3>Set up Ordelis</h3>
      <p>Three things before the first task. Only the first two are required.</p>
      <ul className="setup-list">
        <li>
          <span>
            Add a project folder<small>{projects.length ? `${projects.length} added` : "any folder on this Mac"}</small>
          </span>
        </li>
        <li>
          <span>
            Check at least one agent<small>nothing has been checked yet</small>
          </span>
        </li>
        <li>
          <span>
            Connect an Obsidian vault<small>optional · you can skip this</small>
          </span>
        </li>
      </ul>
      <div className="actions">
        <Button id={id} variant="go" reason={reason} onClick={add}>
          {adding ? "Choose a folder…" : "Add a folder"}
        </Button>
        <Button reason="Agent checks run the CLIs, and this build doesn't connect to the runtime.">Check agents</Button>
      </div>
    </section>
  );
}
