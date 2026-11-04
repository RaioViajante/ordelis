// Pieces shared by the Tasks list, New task, task detail (issue #96), Home, project pages and the sidebar (#98). A saved
// task is only ever a draft or archived: no state here says or suggests that anything is planned, running or finished.
import { useRegistry } from "../app/Registry";
import { useTasks } from "../app/TaskState";
import { hrefFor } from "../app/routes";
import { belongsTo, type ProjectLink, type TaskStatus, type TaskSummary } from "../app/tasks";

const STATUS: Record<TaskStatus, { glyph: string; text: string }> = {
  draft: { glyph: "g-wait", text: "Draft" },
  archived: { glyph: "g-arch", text: "Archived" },
};

/** A glyph and a word, never color alone (Product states board: Draft is a muted pill with a dashed circle). */
export function TaskStatusPill({ status }: { status: TaskStatus }) {
  return (
    <span className="pill p-mute">
      <i className={`g ${STATUS[status].glyph}`} aria-hidden="true" />
      {STATUS[status].text}
    </span>
  );
}

// Labels decided in #86. A task keeps its original project identity and is never moved to another one.
export const LINK_LABEL: Record<Exclude<ProjectLink, "linked">, string> = {
  removed: "Project removed",
  replaced: "Project changed",
  unknown: "Project unavailable",
};
export const LINK_HELP: Record<Exclude<ProjectLink, "linked">, string> = {
  removed: "The project's registration was removed from Ordelis. The task keeps its text and is not moved to another project.",
  replaced:
    "This registration number now belongs to a different folder, after the project list was reset. The task stays with its original project and is not reassigned.",
  unknown: "The project list can't be read right now, so Ordelis can't tell which project this is. The task itself is unaffected.",
};

/** The project as the user knows it: its registered name while linked, otherwise the link state in words. */
export function useProjectLabel(project: TaskSummary["project"]): { text: string; href?: string; problem?: Exclude<ProjectLink, "linked"> } {
  const { projects } = useRegistry();
  if (project.link !== "linked") return { text: LINK_LABEL[project.link], problem: project.link };
  // Only the same registration (id and registration time) lends its name; the name arrives with the project list.
  const registered = projects.find((p) => p.id === project.id && p.registered_at_ms === project.registered_at_ms);
  return registered
    ? { text: registered.name, href: hrefFor("projects", project.id) }
    : { text: `Project ${project.id}` };
}

export interface TaskCounts {
  drafts: number;
  archived: number;
}

/**
 * Counts from the saved tasks, or null while they are loading, could not be read, or cannot exist (browser preview),
 * so a count is never a guess and a failure never reads as zero. With a registration, only its own tasks count: a
 * task saved for a removed registration, or for an earlier one with the same id, counts nowhere.
 */
export function useTaskCounts(): ((project?: { id: string; registered_at_ms: number }) => TaskCounts) | null {
  const { desktop, loading, listError, tasks } = useTasks();
  if (!desktop || loading || listError) return null;
  return (project) => {
    const mine = project ? tasks.filter((t) => belongsTo(t, project)) : tasks;
    const drafts = mine.filter((t) => t.status === "draft").length;
    return { drafts, archived: mine.length - drafts };
  };
}

export const draftsText = (n: number) => `${n} ${n === 1 ? "draft" : "drafts"}`;

/**
 * One saved task in a lifecycle list (Product states board): glyph, title, project and date, and its status pill. On
 * its own project's page the project is not repeated.
 */
export function TaskRow({ task, inProject = false }: { task: TaskSummary; inProject?: boolean }) {
  const project = useProjectLabel(task.project);
  return (
    <li>
      <a className="trow" href={hrefFor("tasks", task.id)}>
        <i className={`g ${task.status === "draft" ? "g-wait" : "g-arch"} trow__g`} aria-hidden="true" />
        <span className="trow__t bidi" dir="auto">
          {task.title}
        </span>
        <span className={`trow__p meta${project.problem ? " trow__p--warn" : ""}`}>
          {!inProject && (
            <>
              <span className="bidi" dir="auto">
                {project.text}
              </span>
              {" · "}
            </>
          )}
          <span>updated {when(task.updated_at_ms)}</span>
        </span>
        <TaskStatusPill status={task.status} />
      </a>
    </li>
  );
}

/** Newest change first. */
export const byUpdated = (a: TaskSummary, b: TaskSummary) => b.updated_at_ms - a.updated_at_ms;

export const when = (ms: number) => new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
