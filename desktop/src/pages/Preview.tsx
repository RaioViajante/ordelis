import type { Location } from "../app/routes";
import { hrefFor } from "../app/routes";
import { useSampleData } from "../app/SampleData";
import { findProject, findTask } from "../fixtures/mock";
import { sessionsForTask, STATE_LABEL } from "../fixtures/sessions";
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
        <dt>Sessions</dt>
        <dd className="flex flex-wrap gap-3 !font-sans">
          {sessionsForTask(task.id).length === 0 && <span className="meta">none in the sample data</span>}
          {sessionsForTask(task.id).map((s) => (
            <a key={s.id} className="link" href={hrefFor("sessions", s.id)}>
              {s.agentName} · {s.id} · {STATE_LABEL[s.state].toLowerCase()}
            </a>
          ))}
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
