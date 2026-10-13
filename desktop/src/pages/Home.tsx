import { useState } from "react";
import { useSampleData } from "../app/SampleData";
import { hrefFor, type Location } from "../app/routes";
import { useWorkspace } from "../app/Workspace";
import { findProject, MOCK_PROJECTS, MOCK_TASKS, MOCK_WAITING } from "../fixtures/mock";
import { Agent, Button, SecHead, Select } from "../components/ui";

export function Home({ go }: { go: (page: Location["page"], param?: string) => void }) {
  const { on } = useSampleData();
  const { project, selectProject, draft, setDraft, clearDraft, contextIds, toggleContext } = useWorkspace();
  const [picking, setPicking] = useState(false);
  const suggestions = findProject(project)?.suggestions ?? [];
  const chosen = contextIds.filter((id) => suggestions.some((s) => s.id === id)).length;
  const blocker = !on
    ? "Sample data is hidden and this build cannot add projects yet."
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
                <option value="">No projects</option>
              )}
            </Select>
            <span className="t-label">
              context:{" "}
              {on && suggestions.length > 0 ? (
                <>
                  <span className="prov prov--you">your notes</span>{" "}
                  <span className="meta">{chosen ? `${chosen} of ${suggestions.length} selected` : `${suggestions.length} suggested`}</span>
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
                <Button reason={on ? "This project has no vault, so there are no notes to suggest." : "No sample notes while sample data is hidden."}>
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
              <legend className="t-label">Sample notes for {project} · nothing is read or sent</legend>
              {suggestions.map((n) => (
                <label key={n.id}>
                  <input type="checkbox" checked={contextIds.includes(n.id)} onChange={() => toggleContext(n.id)} />
                  <span>{n.title}</span>
                  <span className="meta">{n.where}</span>
                </label>
              ))}
            </fieldset>
          )}
        </div>
        <p className="hint">Preview only: nothing here starts an agent. Your draft stays while you move around the app.</p>
      </section>

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

function FirstUse() {
  return (
    <section className="empty max-w-[840px]">
      <h3>Set up Synapse</h3>
      <p>Three things before the first task. Only the first two are required.</p>
      <ul className="setup-list">
        <li>
          <span>
            Add a project folder<small>a git repository or a plain folder</small>
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
        <Button variant="go" reason="Folder access isn't built yet. This build never reads your filesystem.">
          Add a folder
        </Button>
        <Button reason="Agent checks run the CLIs, and this build doesn't connect to the runtime.">Check agents</Button>
      </div>
    </section>
  );
}
