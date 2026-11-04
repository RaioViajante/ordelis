import { useState, type FormEvent } from "react";
import { errorText, useRegistry } from "../app/Registry";
import { taskErrorText, useTasks } from "../app/TaskState";
import { hrefFor, type Location } from "../app/routes";
import { textProblem, type TextProblem } from "../app/tasks";
import { TaskFields } from "../components/TaskFields";
import { Button, Empty, Select } from "../components/ui";
import { STATUS_LABEL } from "./Projects";

type Go = (page: Location["page"], param?: string) => void;

const NO_PLAN = "Planning isn't available yet. Saving a task starts nothing: no agent, no command, no file change.";

// The Composer board's first step, Prepare (issue #96). Steps 2 and 3 are shown as not available, and the only
// action that does anything is Save task.
export function TaskNew({ go }: { go: Go }) {
  const tasks = useTasks();
  const registry = useRegistry();
  const { draft, setDraft } = tasks;
  const [attempted, setAttempted] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const title = draft.title.trim();
  const problem: TextProblem | null = textProblem(title, draft.description);
  const project = registry.projects.find((p) => p.id === draft.projectId);

  async function save(e: FormEvent) {
    e.preventDefault();
    setAttempted(true);
    setFailure(null);
    if (problem || !project || tasks.saving) return;
    const result = await tasks.create({ projectId: project.id, title, description: draft.description });
    if (result.kind === "created") go("tasks", result.task.id);
    // Everything the user typed stays in the form.
    else if (result.kind === "error") setFailure(taskErrorText(result.code));
    else if (result.kind === "unreadable") setFailure(taskErrorText("unreadable"));
  }

  const blocked = !tasks.desktop
    ? "Saving tasks needs the desktop app."
    : tasks.saving
      ? "Saving…"
      : registry.projects.length === 0
        ? "Add a project first."
        : undefined;

  return (
    <div className="col col--1300 tk-new">
      <header className="tk-head">
        <h1 className="h1">New task</h1>
        <ol className="steps" aria-label="Task phases">
          <li className="stp on" aria-current="step">
            <b>1</b>Prepare
          </li>
          <li className="stp" aria-disabled="true">
            <b>2</b>Review plan <span className="sr">(not available yet)</span>
          </li>
          <li className="stp" aria-disabled="true">
            <b>3</b>Run &amp; review <span className="sr">(not available yet)</span>
          </li>
        </ol>
        <span className="meta">Not saved yet</span>
      </header>

      {!tasks.desktop ? (
        <Empty title="Saving tasks needs the desktop app" why="browser preview · no native access">
          This preview can’t save tasks. Open Ordelis to write one.
        </Empty>
      ) : registry.listError ? (
        <div className="notice" role="alert">
          <p>{errorText(registry.listError)} A task needs a project, so it can’t be saved until the list can be read.</p>
        </div>
      ) : !registry.loading && registry.projects.length === 0 ? (
        <Empty
          title="Add a project first"
          why="projects: 0"
          actions={
            <a className="btn btn--go" href={hrefFor("projects")}>
              Go to Projects
            </a>
          }
        >
          Every task belongs to one of your project folders. Add a folder, then come back to write the task.
        </Empty>
      ) : (
        <div className="tk-lay">
          <form className="tk-form" aria-label="Prepare task" onSubmit={save} noValidate>
            <div className="tk-field">
              <label className="lb" htmlFor="tk-project">
                Project
              </label>
              <Select
                id="tk-project"
                className="tk-project"
                value={draft.projectId}
                disabled={registry.loading}
                onChange={(e) => setDraft({ ...draft, projectId: e.target.value })}
                aria-invalid={attempted && !project ? true : undefined}
                aria-describedby={attempted && !project ? "tk-project-problem" : undefined}
              >
                <option value="">{registry.loading ? "Loading projects…" : "Choose a project…"}</option>
                {registry.projects.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                    {p.status === "available" ? "" : ` (${STATUS_LABEL[p.status].text.toLowerCase()})`}
                  </option>
                ))}
              </Select>
              {attempted && !project && (
                <p id="tk-project-problem" className="tk-problem" role="alert">
                  Choose the project this task belongs to.
                </p>
              )}
            </div>

            <TaskFields
              title={draft.title}
              description={draft.description}
              onTitle={(value) => setDraft({ ...draft, title: value })}
              onDescription={(value) => setDraft({ ...draft, description: value })}
              problem={attempted ? problem : null}
              readOnly={tasks.saving}
            />

            {failure && (
              <div className="notice" role="alert">
                <p>{failure} Your text is still here.</p>
              </div>
            )}

            <div className="tk-actions">
              <a className="btn btn--ghost" href={hrefFor("tasks")}>
                Cancel
              </a>
              <span className="sp" />
              <Button reason={NO_PLAN}>Draft plan</Button>
              {blocked ? (
                <Button variant="go" reason={blocked}>
                  {tasks.saving ? "Saving…" : "Save task"}
                </Button>
              ) : (
                <button type="submit" className="btn btn--go">
                  Save task
                </button>
              )}
            </div>
            <p className="cap">Your text stays here while you move around Ordelis, until you save it or quit.</p>
          </form>

          <aside className="tk-aside card" aria-label="What happens next">
            <h2 className="lb">What happens next</h2>
            <ol className="tk-next">
              <li>
                <b>1</b>
                <span>
                  <strong>Saved on this Mac</strong>
                  The task is kept with the project you chose, as a draft. You can edit, archive or restore it.
                </span>
              </li>
              <li>
                <b>2</b>
                <span>
                  <strong>Planning isn’t available yet</strong>A later version will propose a plan for you to review.
                </span>
              </li>
              <li>
                <b>3</b>
                <span>
                  <strong>Nothing runs</strong>No agent starts, no command runs and no file in the project changes.
                </span>
              </li>
            </ol>
          </aside>
        </div>
      )}
    </div>
  );
}
