import { useEffect, useRef, useState, type FormEvent } from "react";
import { taskErrorText, useTasks, type TaskEdit } from "../app/TaskState";
import { hrefFor } from "../app/routes";
import { textProblem, type Task, type UpdateResult } from "../app/tasks";
import { LINK_HELP, TaskStatusPill, useProjectLabel, when } from "../components/TaskBits";
import { TaskFields } from "../components/TaskFields";
import { Button, Empty } from "../components/ui";

const NO_PLAN = "Planning isn't available yet. This task stays a draft and nothing runs.";

type Loaded = { kind: "loading" } | { kind: "found"; task: Task } | { kind: "not_found" } | { kind: "failed"; text: string };

// The Task detail board without its execution tabs, inspector and plan, none of which exist (issue #96). The shell
// keys it by task id, so another task always starts from fresh state.
export function TaskDetail({ id }: { id: string }) {
  const tasks = useTasks();
  const [loaded, setLoaded] = useState<Loaded>({ kind: "loading" });
  // A newer version found when saving: shown instead of the stale one, while the user's edits stay open.
  const [conflict, setConflict] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const edit = tasks.edits[id];
  const [attempted, setAttempted] = useState(false);
  const editRef = useRef<HTMLButtonElement>(null);
  const formRef = useRef<HTMLFormElement>(null);
  const { get } = tasks;

  useEffect(() => {
    let current = true;
    void get(id).then((result) => {
      if (!current) return;
      if (result.kind === "found") setLoaded({ kind: "found", task: result.task });
      else if (result.kind === "not_found") setLoaded({ kind: "not_found" });
      else if (result.kind === "preview") setLoaded({ kind: "failed", text: "Saved tasks need the desktop app." });
      else setLoaded({ kind: "failed", text: taskErrorText(result.kind === "error" ? result.code : "unreadable") });
    });
    return () => {
      current = false;
    };
  }, [id, get]);

  // When an edit opens, the title field takes focus; when it closes, focus returns to Edit.
  const editing = edit !== undefined;
  const wasEditing = useRef(editing);
  useEffect(() => {
    if (editing && !wasEditing.current) formRef.current?.querySelector("input")?.focus();
    if (!editing && wasEditing.current) editRef.current?.focus();
    wasEditing.current = editing;
  }, [editing]);

  if (loaded.kind === "loading") {
    return (
      <div className="col col--1100">
        <p className="meta" role="status">
          Loading the task…
        </p>
      </div>
    );
  }
  if (loaded.kind !== "found") {
    return (
      <div className="col col--1100">
        <h1 className="h1">Task</h1>
        {loaded.kind === "not_found" ? (
          <Empty
            title="Task not found"
            why={`no saved task has the id “${id}”`}
            actions={
              <a className="btn" href={hrefFor("tasks")}>
                Back to Tasks
              </a>
            }
          >
            This link points at a task that isn’t saved on this Mac.
          </Empty>
        ) : (
          <div className="notice" role="alert">
            <p>{loaded.text}</p>
          </div>
        )}
      </div>
    );
  }

  const task = loaded.task;

  function settle(result: UpdateResult, onUpdated: (task: Task) => void) {
    if (result.kind === "updated") {
      setLoaded({ kind: "found", task: result.task });
      setConflict(false);
      onUpdated(result.task);
    } else if (result.kind === "conflict") {
      // Nothing was written. The newer version is shown; the user's edits, if any, are kept for review.
      setLoaded({ kind: "found", task: result.task });
      setConflict(true);
    } else if (result.kind === "not_found") {
      setMessage("This task no longer exists, so nothing was saved. Your edits are still shown.");
    } else if (result.kind === "error") {
      setMessage(taskErrorText(result.code));
    } else if (result.kind === "unreadable") {
      setMessage(taskErrorText("unreadable"));
    }
  }

  async function setStatus(status: Task["status"]) {
    if (tasks.saving) return;
    setMessage(null);
    const result = await tasks.update({
      id: task.id,
      revision: task.revision,
      title: task.title,
      description: task.description,
      status,
    });
    settle(result, () => {});
  }

  function startEdit() {
    setAttempted(false);
    setMessage(null);
    tasks.setEdit(task.id, { title: task.title, description: task.description, revision: task.revision });
  }

  async function saveEdit(e: FormEvent, current: TaskEdit, revision: number) {
    e.preventDefault();
    setAttempted(true);
    setMessage(null);
    const title = current.title.trim();
    if (textProblem(title, current.description) || tasks.saving) return;
    const result = await tasks.update({ id: task.id, revision, title, description: current.description, status: task.status });
    settle(result, () => tasks.setEdit(task.id, null));
  }

  return (
    <div className="col col--1100 tk-detail">
      <TaskHeader task={task} />

      <div className="tk-titlebar">
        <h1 className="h1 bidi" dir="auto">
          {task.title}
        </h1>
        <TaskStatusPill status={task.status} />
        <span className="sp" />
        {!editing && (
          <Button ref={editRef} small onClick={startEdit} reason={tasks.saving ? "Saving…" : undefined}>
            Edit
          </Button>
        )}
        {task.status === "draft" ? (
          <Button small onClick={() => void setStatus("archived")} reason={editing ? "Save or discard your edits first." : tasks.saving ? "Saving…" : undefined}>
            Archive
          </Button>
        ) : (
          <Button small onClick={() => void setStatus("draft")} reason={editing ? "Save or discard your edits first." : tasks.saving ? "Saving…" : undefined}>
            Restore
          </Button>
        )}
      </div>

      {conflict && (
        <div className="notice" role="alert">
          <p>
            This task was changed elsewhere after you opened it, so nothing was saved. The latest saved version is shown
            {editing ? " below your edits, which are still here. Review it before saving again." : "."}
          </p>
        </div>
      )}
      {message && (
        <div className="notice" role="alert">
          <p>{message}</p>
        </div>
      )}

      {editing && (
        <form ref={formRef} className="tk-form tk-edit" aria-label="Edit task" onSubmit={(e) => void saveEdit(e, edit, conflict ? task.revision : edit.revision)} noValidate>
          <TaskFields
            title={edit.title}
            description={edit.description}
            onTitle={(value) => tasks.setEdit(task.id, { ...edit, title: value })}
            onDescription={(value) => tasks.setEdit(task.id, { ...edit, description: value })}
            problem={attempted ? textProblem(edit.title.trim(), edit.description) : null}
            readOnly={tasks.saving}
          />
          <div className="tk-actions">
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => {
                tasks.setEdit(task.id, null);
                setConflict(false);
              }}
            >
              Discard edits
            </button>
            <span className="sp" />
            {tasks.saving ? (
              <Button variant="go" reason="Saving…">
                Saving…
              </Button>
            ) : (
              <button type="submit" className="btn btn--go">
                {conflict ? "Save my version over it" : "Save changes"}
              </button>
            )}
          </div>
        </form>
      )}

      <TaskFacts task={task} />

      <section aria-labelledby="tk-desc" className="flex flex-col gap-2">
        <h2 id="tk-desc" className="lb">
          {editing ? "Saved description" : "Description"}
        </h2>
        {task.description ? (
          // Plain text only: line breaks are kept, nothing is parsed as markup or links.
          <p className="preview-text bidi" dir="auto">
            {task.description}
          </p>
        ) : (
          <p className="meta">No description.</p>
        )}
      </section>

      <div className="notice notice--quiet">
        <p>Planning isn’t available yet. This task is saved as a draft, and nothing has run.</p>
        <span className="sp" />
        <Button small reason={NO_PLAN}>
          Draft plan
        </Button>
      </div>
    </div>
  );
}

function TaskHeader({ task }: { task: Task }) {
  const project = useProjectLabel(task.project);
  return (
    <nav className="meta tk-crumbs" aria-label="Breadcrumb">
      {project.href ? (
        <a className="link bidi" dir="auto" href={project.href}>
          {project.text}
        </a>
      ) : (
        <span>{project.text}</span>
      )}
      <span aria-hidden="true">›</span>
      <a className="link" href={hrefFor("tasks")}>
        Tasks
      </a>
    </nav>
  );
}

function TaskFacts({ task }: { task: Task }) {
  const project = useProjectLabel(task.project);
  return (
    <dl className="kv2 tk-facts">
      <dt>Project</dt>
      <dd className="!font-sans">
        <span className={project.problem ? "tk-warn" : undefined}>
          <span className="bidi" dir="auto">
            {project.text}
          </span>
        </span>
        {project.problem && <span className="meta block">{LINK_HELP[project.problem]}</span>}
      </dd>
      <dt>Created</dt>
      <dd className="!font-sans">{when(task.created_at_ms)}</dd>
      <dt>Updated</dt>
      <dd className="!font-sans">{when(task.updated_at_ms)}</dd>
    </dl>
  );
}
