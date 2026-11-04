// Pieces shared by Home's composer and New task (issue #98): the project picker and the discard action.
import { useRef, useState } from "react";
import { useRegistry } from "../app/Registry";
import { useTasks } from "../app/TaskState";
import type { useComposer } from "../app/useComposer";
import { STATUS_LABEL } from "../pages/Projects";
import { ConfirmDialog } from "./ConfirmDialog";
import { Select } from "./ui";

type Composer = ReturnType<typeof useComposer>;

/** Registered projects only. A registration that was removed or replaced is no longer offered or kept selected. */
export function ProjectSelect({ composer, id, className }: { composer: Composer; id: string; className?: string }) {
  const registry = useRegistry();
  return (
    <Select
      id={id}
      className={className}
      value={composer.project?.id ?? ""}
      disabled={!registry.desktop || registry.loading || composer.saving}
      onChange={(e) => composer.chooseProject(e.target.value)}
      aria-invalid={composer.projectMissing ? true : undefined}
      aria-describedby={composer.projectMissing ? `${id}-problem` : undefined}
    >
      <option value="">
        {!registry.desktop ? "No projects in the browser preview" : registry.loading ? "Loading projects…" : "Choose a project…"}
      </option>
      {registry.projects.map((p) => (
        <option key={p.id} value={p.id}>
          {p.name}
          {p.status === "available" ? "" : ` (${STATUS_LABEL[p.status].text.toLowerCase()})`}
        </option>
      ))}
    </Select>
  );
}

export function ProjectProblem({ composer, id }: { composer: Composer; id: string }) {
  if (!composer.projectMissing) return null;
  return (
    <p id={`${id}-problem`} className="tk-problem" role="alert">
      Choose the project this task belongs to.
    </p>
  );
}

/**
 * Throws the draft away, only after the user confirms. Leaving the page never does: the draft stays in memory until it
 * is saved, discarded here, or Ordelis quits.
 */
export function DiscardDraft({ composer, onDiscarded }: { composer: Composer; onDiscarded?: () => void }) {
  const { discardDraft } = useTasks();
  const [confirming, setConfirming] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  if (!composer.hasText && !confirming) return null;
  return (
    <>
      <button
        ref={button}
        type="button"
        className="btn btn--ghost btn--sm"
        aria-disabled={composer.saving || undefined}
        onClick={() => !composer.saving && setConfirming(true)}
      >
        Discard draft…
      </button>
      {confirming && (
        <ConfirmDialog
          title="Discard this draft?"
          confirmLabel="Discard"
          onCancel={() => {
            setConfirming(false);
            button.current?.focus();
          }}
          onConfirm={() => {
            setConfirming(false);
            discardDraft();
            onDiscarded?.();
          }}
        >
          <p>The title and description you wrote are deleted. Nothing was saved, so this can’t be undone.</p>
        </ConfirmDialog>
      )}
    </>
  );
}
