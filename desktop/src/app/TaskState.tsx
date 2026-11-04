import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  createTask,
  getTask,
  inDesktop,
  listTasks,
  updateTask,
  type CreateResult,
  type GetResult,
  type NewTask,
  type TaskChange,
  type TaskErrorCode,
  type TaskSummary,
  type TextProblem,
  type UpdateResult,
} from "./tasks";
import { useWorkspace } from "./Workspace";

/** Why the list could not be read. "unreadable" is an answer outside the Rust contract. */
export type TaskListError = TaskErrorCode | "unreadable";

// The UI words every code itself, so no native text is shown. None of these contains task text.
const ERROR_TEXT: Record<TaskListError, string> = {
  invalid_input: "Ordelis refused those task details. Nothing was saved.",
  project_not_found: "That project is no longer registered. Choose another project; nothing was saved.",
  project_unavailable: "The project list can't be read right now, so the task can't be linked to a project. Nothing was saved.",
  tasks_full: "The task list is full (500 tasks). Nothing was saved.",
  tasks_corrupt:
    "Your saved tasks could not be read. The task file was kept exactly as it was, and nothing was deleted or replaced. A way to recover it will come in a later version.",
  tasks_newer_version: "Your saved tasks were written by a newer version of Ordelis. They were kept unchanged. Update Ordelis to see them.",
  tasks_unavailable: "Your saved tasks are unavailable right now. Nothing was changed.",
  unsupported_platform: "Saved tasks are only supported on macOS for now.",
  unreadable: "Ordelis could not read the answer for your tasks. Nothing was changed.",
};
export const taskErrorText = (code: TaskListError) => ERROR_TEXT[code];

const PROBLEM_TEXT: Record<TextProblem, string> = {
  title_empty: "Give the task a title.",
  title_too_long: "The title is too long. Keep it under 256 bytes (about 256 Latin letters, fewer for other scripts).",
  title_spaces: "The title can't start or end with spaces.",
  title_control: "The title contains a control character, such as a tab or a line break. Remove it.",
  description_control: "The description contains a control character other than line breaks and tabs. Remove it.",
  too_long: "The task is too long. The title and description together must fit in 4,094 bytes.",
};
export const problemText = (problem: TextProblem) => PROBLEM_TEXT[problem];

/** What the composer holds while the user writes. In memory only: it survives moving around, not a restart. */
export interface ComposerDraft {
  projectId: string;
  title: string;
  description: string;
}
const EMPTY_DRAFT: ComposerDraft = { projectId: "", title: "", description: "" };

/** Unsaved edits of one task, with the revision they started from. In memory only, like the composer draft. */
export interface TaskEdit {
  title: string;
  description: string;
  revision: number;
}

interface TaskState {
  /** False in a browser preview, where no native call is ever attempted. */
  desktop: boolean;
  /** The first list is in flight. */
  loading: boolean;
  /** The last list Rust answered, in creation order. Held in memory; Rust owns the persistent copy. */
  tasks: TaskSummary[];
  /** The last list failed. The previous list is not shown as current. */
  listError: TaskListError | null;
  /** A create or update is in flight. Only one runs at a time. */
  saving: boolean;
  reload: () => Promise<void>;
  get: (id: string) => Promise<GetResult>;
  create: (input: NewTask) => Promise<CreateResult>;
  update: (change: TaskChange) => Promise<UpdateResult>;
  draft: ComposerDraft;
  setDraft: (draft: ComposerDraft) => void;
  edits: Record<string, TaskEdit>;
  /** `null` drops the edits of that task, after a save or when the user discards them. */
  setEdit: (id: string, edit: TaskEdit | null) => void;
}

const TaskContext = createContext<TaskState | null>(null);

// The Tasks list, New task and task detail read the same state. Rust owns persistence: nothing here is written
// anywhere, and the list is read again after every change, including a conflict.
export function TasksProvider({ children }: { children: ReactNode }) {
  const { announce } = useWorkspace();
  const [desktop] = useState(inDesktop);
  const [list, setList] = useState<{ loading: boolean; tasks: TaskSummary[]; error: TaskListError | null }>({
    loading: desktop,
    tasks: [],
    error: null,
  });
  const [saving, setSaving] = useState(false);
  const [draft, setDraft] = useState<ComposerDraft>(EMPTY_DRAFT);
  const [edits, setEdits] = useState<Record<string, TaskEdit>>({});
  const setEdit = useCallback((id: string, edit: TaskEdit | null) => {
    setEdits((current) => {
      const next = { ...current };
      if (edit) next[id] = edit;
      else delete next[id];
      return next;
    });
  }, []);
  const mounted = useRef(false);
  // A second submit while one is in flight shares its answer instead of reaching Rust again.
  const inflight = useRef<Promise<CreateResult | UpdateResult> | null>(null);

  const reload = useCallback(
    () =>
      listTasks().then((result) => {
        if (!mounted.current) return;
        if (result.kind === "ok") setList({ loading: false, tasks: result.tasks, error: null });
        else if (result.kind === "preview") setList({ loading: false, tasks: [], error: null });
        // A failed list is not shown as an empty one, and an earlier list is not shown as current.
        else setList({ loading: false, tasks: [], error: result.kind === "error" ? result.code : "unreadable" });
      }),
    [],
  );

  useEffect(() => {
    mounted.current = true;
    if (desktop) void reload();
    return () => {
      mounted.current = false;
    };
  }, [desktop, reload]);

  const mutate = useCallback(
    <T extends CreateResult | UpdateResult>(work: () => Promise<T>): Promise<T> => {
      if (inflight.current) return inflight.current as Promise<T>;
      setSaving(true);
      const running = work()
        .then(async (result) => {
          if (result.kind !== "preview") await reload();
          return result;
        })
        .finally(() => {
          inflight.current = null;
          if (mounted.current) setSaving(false);
        });
      inflight.current = running;
      return running;
    },
    [reload],
  );

  const create = useCallback(
    (input: NewTask) =>
      mutate(() => createTask(input)).then((result) => {
        if (result.kind === "created") {
          setDraft(EMPTY_DRAFT);
          announce("Task saved as a draft. Nothing was started.");
        }
        return result;
      }),
    [mutate, announce],
  );

  const update = useCallback(
    (change: TaskChange) =>
      mutate(() => updateTask(change)).then((result) => {
        if (result.kind === "updated") announce(change.status === "archived" ? "Task saved and archived." : "Task saved.");
        if (result.kind === "conflict") announce("This task was changed elsewhere. Nothing was saved.");
        return result;
      }),
    [mutate, announce],
  );

  const value = useMemo(
    () => ({
      desktop,
      loading: list.loading,
      tasks: list.tasks,
      listError: list.error,
      saving,
      reload,
      get: getTask,
      create,
      update,
      draft,
      setDraft,
      edits,
      setEdit,
    }),
    [desktop, list, saving, reload, create, update, draft, edits, setEdit],
  );
  return <TaskContext.Provider value={value}>{children}</TaskContext.Provider>;
}

export function useTasks(): TaskState {
  const ctx = useContext(TaskContext);
  if (!ctx) throw new Error("useTasks must be used inside TasksProvider");
  return ctx;
}
