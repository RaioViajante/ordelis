import { useState } from "react";
import { taskErrorText, useTasks } from "../app/TaskState";
import { hrefFor } from "../app/routes";
import type { TaskStatus, TaskSummary } from "../app/tasks";
import { TaskStatusPill, useProjectLabel, when } from "../components/TaskBits";
import { Button, Empty } from "../components/ui";

// Saved tasks only (issue #96). Sample tasks stay in Home's labelled sample section and are never listed here.
export function Tasks() {
  const { desktop, loading, tasks, listError, reload } = useTasks();
  const [view, setView] = useState<TaskStatus>("draft");
  const shown = tasks.filter((t) => t.status === view).sort((a, b) => b.updated_at_ms - a.updated_at_ms);
  const count = (status: TaskStatus) => tasks.filter((t) => t.status === status).length;

  return (
    <div className="col col--1100">
      <div className="flex items-end gap-5">
        <h1 className="h1">Tasks</h1>
        <span className="sp" />
        {desktop ? (
          <a className="btn btn--go" href={hrefFor("tasks", "new")}>
            New task
          </a>
        ) : (
          <Button variant="go" reason="Saving tasks needs the desktop app.">
            New task
          </Button>
        )}
      </div>
      <p className="hint max-w-[760px]">
        A task is a note about work you want done, saved on this Mac with its project. Saving one starts nothing: planning
        and agents aren’t available yet.
      </p>

      {!desktop ? (
        <Empty title="Saved tasks need the desktop app" why="browser preview · no native access">
          This preview can’t read or save tasks. Open Ordelis to see yours.
        </Empty>
      ) : listError ? (
        // Never an empty list: the tasks exist but could not be read.
        <div className="notice" role="alert">
          <p>{taskErrorText(listError)}</p>
          <span className="sp" />
          <Button small onClick={() => void reload()}>
            Try again
          </Button>
        </div>
      ) : loading ? (
        <p className="meta" role="status">
          Loading your tasks…
        </p>
      ) : tasks.length === 0 ? (
        <Empty
          title="No saved tasks yet"
          why="tasks: 0"
          actions={
            <a className="btn btn--go" href={hrefFor("tasks", "new")}>
              New task
            </a>
          }
        >
          Write down the work you want done in one of your projects. It stays a draft: nothing is planned or run.
        </Empty>
      ) : (
        <section aria-label="Saved tasks" className="flex flex-col gap-4">
          <div className="seg" role="group" aria-label="Show">
            {(["draft", "archived"] as const).map((status) => (
              <button key={status} type="button" aria-pressed={view === status} onClick={() => setView(status)}>
                {status === "draft" ? "Drafts" : "Archived"} <span className="meta">{count(status)}</span>
              </button>
            ))}
          </div>
          {shown.length === 0 ? (
            <p className="meta">{view === "draft" ? "No drafts. Archived tasks are under Archived." : "Nothing is archived."}</p>
          ) : (
            <ul className="tlist card" aria-label={view === "draft" ? "Drafts" : "Archived tasks"}>
              {shown.map((task) => (
                <TaskRow key={task.id} task={task} />
              ))}
            </ul>
          )}
        </section>
      )}
    </div>
  );
}

function TaskRow({ task }: { task: TaskSummary }) {
  const project = useProjectLabel(task.project);
  return (
    <li>
      <a className="trow" href={hrefFor("tasks", task.id)}>
        <i className={`g ${task.status === "draft" ? "g-wait" : "g-arch"} trow__g`} aria-hidden="true" />
        <span className="trow__t bidi" dir="auto">
          {task.title}
        </span>
        <span className={`trow__p meta${project.problem ? " trow__p--warn" : ""}`}>
          <span className="bidi" dir="auto">
            {project.text}
          </span>
          {" · "}
          <span>updated {when(task.updated_at_ms)}</span>
        </span>
        <TaskStatusPill status={task.status} />
      </a>
    </li>
  );
}
