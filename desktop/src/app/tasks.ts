// The UI's adapter for saved tasks (issue #96). It is the only module that names or invokes the task commands, whose
// contracts are in docs/implementation/desktop-tasks.md: list_tasks takes nothing, get_task takes a task id,
// create_task takes the chosen registration (its id and registration time) and text, update_task takes a task id, a
// revision, text and a status. None takes a path. Every answer is checked against that contract before the UI may
// show it, input Rust would refuse is never sent, and the native message text is never shown: the UI words each error
// code itself.
import { invoke, isTauri } from "@tauri-apps/api/core";

export const TASK_ERROR_CODES = [
  "invalid_input",
  "project_not_found",
  "project_unavailable",
  "tasks_full",
  "tasks_corrupt",
  "tasks_newer_version",
  "tasks_unavailable",
  "unsupported_platform",
] as const;
export type TaskErrorCode = (typeof TASK_ERROR_CODES)[number];

/** The only stored statuses. A saved task is never planned, running or finished. */
export const TASK_STATUSES = ["draft", "archived"] as const;
export type TaskStatus = (typeof TASK_STATUSES)[number];

/** How a task's saved project identity relates to the registry now, computed by Rust on every read. */
export const LINKS = ["linked", "removed", "replaced", "unknown"] as const;
export type ProjectLink = (typeof LINKS)[number];

// Mirrors TaskSummary and Task in src-tauri/src/tasks.rs. `project` is the registration the task was saved for (its
// id and registration time, never a path) and how it relates to the registry now.
export interface TaskSummary {
  id: string;
  title: string;
  status: TaskStatus;
  project: { id: string; registered_at_ms: number; link: ProjectLink };
  created_at_ms: number;
  updated_at_ms: number;
  revision: number;
}
export interface Task extends TaskSummary {
  description: string;
}

// "unreadable" covers anything outside the contract. "preview" means the page runs outside the desktop app.
type Failure = { kind: "error"; code: TaskErrorCode } | { kind: "unreadable" } | { kind: "preview" };
export type ListResult = { kind: "ok"; tasks: TaskSummary[] } | Failure;
export type GetResult = { kind: "found"; task: Task } | { kind: "not_found" } | Failure;
export type CreateResult = { kind: "created"; task: Task } | Failure;
export type UpdateResult = { kind: "updated"; task: Task } | { kind: "conflict"; task: Task } | { kind: "not_found" } | Failure;

export interface NewTask {
  projectId: string;
  /** The chosen registration's time. Rust refuses the task if the id now names a different registration (#98). */
  registeredAtMs: number;
  title: string;
  description: string;
}
export interface TaskChange {
  id: string;
  revision: number;
  title: string;
  description: string;
  status: TaskStatus;
}

// Rust's limits (src-tauri/src/tasks/records.rs). Lengths are UTF-8 bytes, as Rust counts them.
export const MAX_TITLE_BYTES = 256;
/** Title, the two newlines that separate it from the description, and the description. */
export const MAX_TEXT_BYTES = 4096;
// Rust's char::is_control: U+0000–U+001F and U+007F–U+009F.
const isControl = (code: number) => code <= 0x1f || (code >= 0x7f && code <= 0x9f);
function hasControl(text: string, allowed = ""): boolean {
  for (const ch of text) if (isControl(ch.codePointAt(0)!) && !allowed.includes(ch)) return true;
  return false;
}

export const bytes = (text: string) => new TextEncoder().encode(text).length;

/** Why Rust would refuse this text, or null. The title is expected trimmed already; nothing is ever cut. */
export type TextProblem = "title_empty" | "title_too_long" | "title_spaces" | "title_control" | "description_control" | "too_long";
export function textProblem(title: string, description: string): TextProblem | null {
  if (title.length === 0) return "title_empty";
  if (bytes(title) > MAX_TITLE_BYTES) return "title_too_long";
  if (title.trim() !== title) return "title_spaces";
  if (hasControl(title)) return "title_control";
  if (hasControl(description, "\n\t")) return "description_control";
  if (bytes(title) + 2 + bytes(description) > MAX_TEXT_BYTES) return "too_long";
  return null;
}

/**
 * Whether a task belongs to this registration: the same id and registration time, and Rust found that registration
 * when it listed the task. A task saved for an earlier registration with the same id is never counted here.
 */
export function belongsTo(task: TaskSummary, project: { id: string; registered_at_ms: number }): boolean {
  return task.project.link === "linked" && task.project.id === project.id && task.project.registered_at_ms === project.registered_at_ms;
}

export function inDesktop(): boolean {
  return isTauri();
}

const PREVIEW: Failure = { kind: "preview" };
const UNREADABLE: Failure = { kind: "unreadable" };
const INVALID: Failure = { kind: "error", code: "invalid_input" };
const TASK_ID = /^t[1-9][0-9]{0,9}$/;
const PROJECT_ID = /^p[1-9][0-9]{0,9}$/;
const MAX_SAFE = Number.MAX_SAFE_INTEGER;

// The list is shared while it is in flight, so Strict Mode's second effect run is free. Nothing is cached.
let pendingList: Promise<ListResult> | null = null;

export function listTasks(): Promise<ListResult> {
  if (!inDesktop()) return Promise.resolve(PREVIEW);
  pendingList ??= invoke("list_tasks")
    .then(readList, readError)
    .finally(() => {
      pendingList = null;
    });
  return pendingList;
}

export function getTask(id: string): Promise<GetResult> {
  if (!inDesktop()) return Promise.resolve(PREVIEW);
  if (!TASK_ID.test(id)) return Promise.resolve(INVALID);
  return invoke("get_task", { id }).then(readGet, readError);
}

export function createTask(input: NewTask): Promise<CreateResult> {
  if (!inDesktop()) return Promise.resolve(PREVIEW);
  const { projectId, registeredAtMs, title, description } = input;
  if (!PROJECT_ID.test(projectId) || !time(registeredAtMs) || textProblem(title, description)) return Promise.resolve(INVALID);
  return invoke("create_task", { project_id: projectId, expected_registered_at_ms: registeredAtMs, title, description }).then(readCreate, readError);
}

export function updateTask(change: TaskChange): Promise<UpdateResult> {
  if (!inDesktop()) return Promise.resolve(PREVIEW);
  const { id, revision, title, description, status } = change;
  const ok =
    TASK_ID.test(id) &&
    Number.isSafeInteger(revision) &&
    revision >= 1 &&
    TASK_STATUSES.some((s) => s === status) &&
    !textProblem(title, description);
  if (!ok) return Promise.resolve(INVALID);
  return invoke("update_task", { id, revision, title, description, status }).then(readUpdate, readError);
}

type Obj = Record<string, unknown>;

function exact(value: unknown, keys: string[]): value is Obj {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const own = Object.keys(value).sort();
  return own.length === keys.length && [...keys].sort().every((k, i) => own[i] === k);
}

const time = (v: unknown) => Number.isSafeInteger(v) && (v as number) >= 0 && (v as number) <= MAX_SAFE;
const SUMMARY = ["id", "title", "status", "project", "created_at_ms", "updated_at_ms", "revision"];

function readSummaryFields(raw: Obj): boolean {
  const { id, title, status, project, created_at_ms, updated_at_ms, revision } = raw;
  return (
    typeof id === "string" &&
    TASK_ID.test(id) &&
    typeof title === "string" &&
    title.length > 0 &&
    bytes(title) <= MAX_TITLE_BYTES &&
    TASK_STATUSES.some((s) => s === status) &&
    exact(project, ["id", "registered_at_ms", "link"]) &&
    typeof project.id === "string" &&
    PROJECT_ID.test(project.id) &&
    time(project.registered_at_ms) &&
    LINKS.some((l) => l === project.link) &&
    time(created_at_ms) &&
    time(updated_at_ms) &&
    (created_at_ms as number) <= (updated_at_ms as number) &&
    Number.isSafeInteger(revision) &&
    (revision as number) >= 1
  );
}

export function readSummary(raw: unknown): TaskSummary | null {
  return exact(raw, SUMMARY) && readSummaryFields(raw) ? (raw as unknown as TaskSummary) : null;
}

export function readTask(raw: unknown): Task | null {
  if (!exact(raw, [...SUMMARY, "description"]) || !readSummaryFields(raw)) return null;
  const { title, description } = raw;
  const ok = typeof description === "string" && bytes(title as string) + 2 + bytes(description) <= MAX_TEXT_BYTES;
  return ok ? (raw as unknown as Task) : null;
}

export function readList(raw: unknown): ListResult {
  if (!exact(raw, ["schema_version", "tasks"]) || raw.schema_version !== 1 || !Array.isArray(raw.tasks)) return UNREADABLE;
  const tasks = raw.tasks.map(readSummary);
  const ids = new Set(tasks.map((t) => t?.id));
  // A list with an invalid or repeated record is not shown at all: half a list would mislead.
  if (tasks.some((t) => t === null) || ids.size !== tasks.length) return UNREADABLE;
  return { kind: "ok", tasks: tasks as TaskSummary[] };
}

export function readGet(raw: unknown): GetResult {
  if (exact(raw, ["outcome"]) && raw.outcome === "not_found") return { kind: "not_found" };
  if (exact(raw, ["outcome", "task"]) && raw.outcome === "found") {
    const task = readTask(raw.task);
    if (task) return { kind: "found", task };
  }
  return UNREADABLE;
}

export function readCreate(raw: unknown): CreateResult {
  if (exact(raw, ["outcome", "task"]) && raw.outcome === "created") {
    const task = readTask(raw.task);
    if (task) return { kind: "created", task };
  }
  return UNREADABLE;
}

export function readUpdate(raw: unknown): UpdateResult {
  if (exact(raw, ["outcome"]) && raw.outcome === "not_found") return { kind: "not_found" };
  if (exact(raw, ["outcome", "task"]) && (raw.outcome === "updated" || raw.outcome === "conflict")) {
    const task = readTask(raw.task);
    if (task) return { kind: raw.outcome, task };
  }
  return UNREADABLE;
}

// Rust rejects with { code, message }. Only a known code is kept; the message is never shown.
export function readError(raw: unknown): Failure {
  if (exact(raw, ["code", "message"]) && typeof raw.message === "string") {
    const code = TASK_ERROR_CODES.find((c) => c === raw.code);
    if (code) return { kind: "error", code };
  }
  return UNREADABLE;
}
