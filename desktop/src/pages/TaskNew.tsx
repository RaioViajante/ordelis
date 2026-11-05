import { useRef } from "react";
import { errorText, useRegistry } from "../app/Registry";
import { hrefFor, type Location } from "../app/routes";
import { useComposer } from "../app/useComposer";
import { DiscardDraft, ProjectProblem, ProjectSelect } from "../components/Composer";
import { TaskFields } from "../components/TaskFields";
import { Button } from "../components/ui";

type Go = (page: Location["page"], param?: string) => void;

const NO_PLAN = "Planning isn't available yet. Saving a task starts nothing: no agent, no command, no file change.";

// The Composer board's first step, Prepare (issue #96), sharing its draft and save with Home's composer (#98). Steps 2
// and 3 are shown as not available, and the only action that does anything is Save task. The form is always shown, so
// the draft stays visible whatever blocks saving.
export function TaskNew({ go }: { go: Go }) {
  const registry = useRegistry();
  const composer = useComposer(go);
  const { draft } = composer;
  const form = useRef<HTMLFormElement>(null);

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

      {!registry.desktop && (
        <p className="notice notice--quiet">This browser preview can’t save tasks. Open Ordelis to save this one.</p>
      )}
      {registry.desktop && registry.listError && (
        <div className="notice" role="alert">
          <p>{errorText(registry.listError)} A task needs a project, so it can’t be saved until the list can be read.</p>
        </div>
      )}
      {registry.desktop && !registry.loading && !registry.listError && registry.projects.length === 0 && (
        <div className="notice">
          <p>Every task belongs to one of your project folders. Add a folder, then save the task; your text stays here.</p>
          <span className="sp" />
          <a className="btn btn--sm" href={hrefFor("projects")}>
            Go to Projects
          </a>
        </div>
      )}

      <div className="tk-lay">
        <form ref={form} className="tk-form" aria-label="Prepare task" onSubmit={(e) => void composer.save(e)} noValidate>
          <div className="tk-field">
            <label className="lb" htmlFor="tk-project">
              Project
            </label>
            <ProjectSelect composer={composer} id="tk-project" className="tk-project" />
            <ProjectProblem composer={composer} id="tk-project" />
          </div>

          <TaskFields
            title={draft.title}
            description={draft.description}
            onTitle={composer.setTitle}
            onDescription={composer.setDescription}
            problem={composer.problem}
            readOnly={composer.saving}
          />

          {composer.failure && (
            <div className="notice" role="alert">
              <p>{composer.failure} Your text is still here.</p>
            </div>
          )}

          <div className="tk-actions">
            {/* Leaves the page and keeps the draft; discarding is the separate, confirmed action. */}
            <a className="btn btn--ghost" href={hrefFor("tasks")}>
              Back to Tasks
            </a>
            <DiscardDraft composer={composer} onDiscarded={() => form.current?.querySelector("input")?.focus()} />
            {/* The forward actions wrap together, on the right, never Save task alone (#103). */}
            <div className="tk-actions__go">
              <Button reason={NO_PLAN}>Draft plan</Button>
              {composer.blocked ? (
                <Button variant="go" reason={composer.blocked}>
                  {composer.saving ? "Saving…" : "Save task"}
                </Button>
              ) : (
                <button type="submit" className="btn btn--go">
                  Save task
                </button>
              )}
            </div>
          </div>
          <p className="cap">
            This is the same draft as on Home. It stays while you move around Ordelis, until you save or discard it; it isn’t
            kept when Ordelis quits.
          </p>
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
    </div>
  );
}
