import { useState } from "react";
import { useSampleData } from "../app/SampleData";
import type { RouteId } from "../app/routes";
import { MOCK_PROJECTS, MOCK_TASKS, MOCK_WAITING } from "../fixtures/mock";
import { Agent, Button, SecHead, Select } from "../components/ui";

export function Home({ go }: { go: (id: RouteId) => void }) {
  const { on } = useSampleData();
  const [task, setTask] = useState("");
  const [project, setProject] = useState(MOCK_PROJECTS[0].name);

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
            value={task}
            onChange={(e) => setTask(e.target.value)}
            placeholder="Describe a change, a fix, or a question about the code…"
          />
          <div className="ask__bar">
            <label className="sr" htmlFor="proj">
              Project
            </label>
            <Select
              id="proj"
              className="field"
              value={on ? project : ""}
              disabled={!on}
              onChange={(e) => setProject(e.target.value)}
            >
              {on ? (
                MOCK_PROJECTS.map((p) => <option key={p.name}>{p.name}</option>)
              ) : (
                <option value="">No projects</option>
              )}
            </Select>
            <span className="t-label">
              context:{" "}
              {on ? (
                <>
                  <span className="prov prov--you">your notes</span> <span className="meta">3 suggested</span>
                </>
              ) : (
                <span className="meta">none selected</span>
              )}
            </span>
            <span className="sp" />
            <Button variant="go" onClick={() => go("context")}>
              Select context
            </Button>
          </div>
        </div>
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
            <div className="task" key={t.title}>
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
            </div>
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
