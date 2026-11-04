import { useEffect, useRef, useState, type FormEvent } from "react";
import { useRegistry } from "./Registry";
import { taskErrorText, useTasks } from "./TaskState";
import type { Location } from "./routes";
import { textProblem, type TextProblem } from "./tasks";

type Go = (page: Location["page"], param?: string) => void;

/**
 * The task composer behind Home and New task (issue #98). Both pages edit the one in-memory draft, check it with the
 * same rules as Rust, and save it through the provider's single write path, so a second click, or a click on the
 * other page, never reaches Rust while a save is in flight.
 */
export function useComposer(go: Go) {
  const tasks = useTasks();
  const registry = useRegistry();
  const { draft, setDraft } = tasks;
  const [attempted, setAttempted] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  // A save that finishes after the user moved on does not pull them back to the new task.
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const title = draft.title.trim();
  const problem: TextProblem | null = textProblem(title, draft.description);
  // The chosen registration, only while the project list still has it under the same id and registration time.
  const chosen = draft.project;
  const project = chosen
    ? registry.projects.find((p) => p.id === chosen.id && p.registered_at_ms === chosen.registered_at_ms)
    : undefined;

  const blocked = !tasks.desktop
    ? "Saving tasks needs the desktop app."
    : tasks.saving
      ? "Saving…"
      : registry.loading
        ? "Loading your projects…"
        : registry.listError
          ? "The project list can't be read, so the task can't be linked to a project."
          : registry.projects.length === 0
            ? "Add a project first."
            : undefined;

  function chooseProject(id: string) {
    const p = registry.projects.find((r) => r.id === id);
    setDraft({ ...draft, project: p ? { id: p.id, registered_at_ms: p.registered_at_ms } : null });
  }

  async function save(e: FormEvent) {
    e.preventDefault();
    setAttempted(true);
    setFailure(null);
    if (blocked || problem || !project) return;
    const input = { projectId: project.id, registeredAtMs: project.registered_at_ms, title, description: draft.description };
    const result = await tasks.create(input, draft);
    // The project list the user chose from is out of date: read it again, so the stale registration is not offered.
    if (result.kind === "error" && result.code === "project_not_found") void registry.reload();
    if (!mounted.current) return;
    if (result.kind === "created") {
      setAttempted(false);
      go("tasks", result.task.id);
    }
    // Everything the user typed stays in the draft.
    else if (result.kind === "error") setFailure(taskErrorText(result.code));
    else if (result.kind === "unreadable") setFailure(taskErrorText("unreadable"));
  }

  return {
    draft,
    setTitle: (value: string) => setDraft({ ...draft, title: value }),
    setDescription: (value: string) => setDraft({ ...draft, description: value }),
    chooseProject,
    /** The selected registration, if it is still registered. */
    project,
    /** Shown once the user has tried to save. */
    problem: attempted ? problem : null,
    projectMissing: attempted && !project,
    failure,
    blocked,
    saving: tasks.saving,
    /** Whether discarding would lose anything the user typed. */
    hasText: draft.title !== "" || draft.description !== "",
    save,
  };
}
