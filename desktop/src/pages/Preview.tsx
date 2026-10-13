import type { Location } from "../app/routes";
import { hrefFor } from "../app/routes";
import { useSampleData } from "../app/SampleData";
import { useWorkspace } from "../app/Workspace";
import { countText, excerptOf, fmt, sumCounts } from "../app/contextBudget";
import { findProject, findTask, SOURCE_LABEL } from "../fixtures/mock";
import { Agent, Button, Empty, PageTitle, SecHead } from "../components/ui";

type Go = (page: Location["page"], param?: string) => void;

const NO_RUNTIME = "Task planning isn't connected to the runtime yet. This build never starts an agent.";

export function TaskPreview({ id, go }: { id?: string; go: Go }) {
  const { on } = useSampleData();
  const task = on ? findTask(id) : undefined;

  if (!task) {
    return (
      <div className="col col--1300">
        <h1 className="h1">Task preview</h1>
        <Empty
          title="Task not found"
          why={on ? `no sample task has the id “${id ?? ""}”` : "tasks: 0 · sample data is hidden"}
          actions={
            <Button onClick={() => go("home")}>Back to Home</Button>
          }
        >
          {on ? "This link points at a task that isn’t in the sample data." : "There are no tasks to preview while sample data is hidden."}
        </Empty>
      </div>
    );
  }

  return (
    <div className="col col--1300 preview-col">
      <PageTitle lede="A read-only look at a sample task. Nothing is running and no agent is attached to it.">{task.title}</PageTitle>
      <dl className="kv2">
        <dt>Project</dt>
        <dd>
          <a className="link" href={hrefFor("projects", task.project)}>
            {task.project}
          </a>
          {" · "}
          <span className="mono">{findProject(task.project)?.path}</span>
        </dd>
        <dt>Agents</dt>
        <dd className="flex gap-3 !font-sans">
          {task.agents.map((a) => (
            <Agent key={a.id} id={a.id}>
              {a.name}
            </Agent>
          ))}
        </dd>
        <dt>Status</dt>
        <dd className="flex flex-wrap gap-3 !font-sans">
          <span className={`st st--${task.tone}`}>{task.state}</span> <span className="meta">sample, not live</span>
        </dd>
      </dl>
      <section>
        <SecHead title="Request" meta="sample" />
        <p className="preview-text">{task.request}</p>
      </section>
      <div className="flex gap-3">
        <Button onClick={() => go("projects", task.project)}>Back to {task.project}</Button>
        <Button variant="go" reason={NO_RUNTIME}>
          Open terminals
        </Button>
      </div>
    </div>
  );
}

export function DraftPreview({ go }: { go: Go }) {
  const { on } = useSampleData();
  const { project, draft, contextEntries } = useWorkspace();
  const proj = on ? findProject(project) : undefined;

  if (!proj || !draft.trim()) {
    return (
      <div className="col col--1300">
        <h1 className="h1">Task preview</h1>
        <Empty
          title="Nothing to preview"
          why={!proj ? "no project selected" : "the draft is empty"}
          actions={<Button onClick={() => go("home")}>Back to Home</Button>}
        >
          Choose a project and describe a task on Home, then preview it here.
        </Empty>
      </div>
    );
  }

  const total = sumCounts(contextEntries.map((e) => countText(excerptOf(e.text).shown)));
  return (
    <div className="col col--1300 preview-col">
      <PageTitle lede="This is how your draft would read. It is preview only: nothing is sent, planned or run.">Task preview</PageTitle>
      <dl className="kv2">
        <dt>Project</dt>
        <dd>
          {proj.name} · <span className="mono">{proj.path}</span>
        </dd>
        <dt>Context</dt>
        <dd className="!font-sans">{contextEntries.length ? `${contextEntries.length} sample ${contextEntries.length === 1 ? "entry" : "entries"} selected` : "none selected"}</dd>
      </dl>
      <section aria-labelledby="pv-ctx">
        <SecHead title={<span id="pv-ctx">Selected sample context</span>} meta={<a className="link" href={hrefFor("context")}>Change in Context</a>} />
        {contextEntries.length ? (
          <>
            <ul className="rows" aria-label="Selected sample context">
              {contextEntries.map((e) => (
                <li className="r" key={e.id}>
                  <span className={`prov ${e.source === "notes" ? "prov--you" : "prov--hist"}`}>{SOURCE_LABEL[e.source]}</span>
                  <span className="flex-1">{e.title}</span>
                  <span className="mono meta">~{fmt(countText(excerptOf(e.text).shown).estTokens)} est. tok</span>
                </li>
              ))}
            </ul>
            <p className="mono meta mt-2.5">
              {fmt(total.chars)} chars · {fmt(total.bytes)} bytes · ~{fmt(total.estTokens)} est. tok · estimated, not measured
            </p>
          </>
        ) : (
          <p className="hint">No sample context selected. Choose some on Home or in Context.</p>
        )}
        <p className="hint mt-2.5">
          Sample entries only. Previewing does not dispatch a task and does not authorize access to a vault or a provider.
        </p>
      </section>
      <section>
        <SecHead title="Request" meta="your draft" />
        <p className="preview-text">{draft}</p>
      </section>
      <div className="flex gap-3">
        <Button onClick={() => go("home")}>Back to draft</Button>
        <Button variant="go" reason={NO_RUNTIME}>
          Plan with agents
        </Button>
      </div>
    </div>
  );
}
