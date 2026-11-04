// Pieces shared by the Tasks list, New task and task detail (issue #96). A saved task is only ever a draft or
// archived: no state here says or suggests that anything is planned, running or finished.
import { useRegistry } from "../app/Registry";
import { hrefFor } from "../app/routes";
import type { ProjectLink, TaskStatus, TaskSummary } from "../app/tasks";

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
  const registered = projects.find((p) => p.id === project.id);
  // Rust confirmed the link; the name arrives with the project list.
  return { text: registered?.name ?? `Project ${project.id}`, href: hrefFor("projects", project.id) };
}

export const when = (ms: number) => new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
