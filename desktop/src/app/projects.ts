// The UI's adapter for the local project registry (issue #74). It is the only module that names or invokes
// the project commands, which are fixed here and take no path: list_projects and add_project take nothing,
// remove_project takes only a registration id. The folder is chosen in a native panel that Rust opens.
// The registry's reset command is deliberately not reachable from the UI. Every answer is checked against the Rust contract
// in docs/implementation/desktop-local-projects.md before the UI may show it, and the native message text is
// never shown: the UI words each error code itself.
import { invoke, isTauri } from "@tauri-apps/api/core";

export const ERROR_CODES = [
  "not_allowed",
  "selection_in_progress",
  "not_a_directory",
  "folder_refused",
  "folder_changed",
  "folder_inaccessible",
  "path_unsupported",
  "registry_full",
  "invalid_input",
  "registry_corrupt",
  "registry_newer_version",
  "registry_unavailable",
  "unsupported_platform",
] as const;
export type ProjectErrorCode = (typeof ERROR_CODES)[number];

export const STATUSES = ["available", "missing", "inaccessible", "changed"] as const;
export type ProjectStatus = (typeof STATUSES)[number];

// Mirrors Project in src-tauri/src/projects.rs. Device and inode numbers never leave Rust.
export interface Project {
  id: string;
  name: string;
  path_display: string;
  status: ProjectStatus;
  registered_at_ms: number;
}

// "unreadable" covers anything outside the contract: a changed schema, an ACL rejection, an unknown error.
// "preview" means the page runs outside the desktop app, where no native call is attempted.
type Failure = { kind: "error"; code: ProjectErrorCode } | { kind: "unreadable" } | { kind: "preview" };
export type ListResult = { kind: "ok"; projects: Project[] } | Failure;
export type AddResult =
  | { kind: "added"; project: Project }
  | { kind: "already_registered"; project: Project }
  | { kind: "cancelled" }
  | Failure;
export type RemoveResult = { kind: "removed" } | { kind: "not_found" } | Failure;

/** Whether the page runs inside the Tauri webview at all. It says nothing about whether a command is allowed. */
export function inDesktop(): boolean {
  return isTauri();
}

const PREVIEW: Failure = { kind: "preview" };
const UNREADABLE: Failure = { kind: "unreadable" };

// One native panel at a time: a second add while one is open shares its answer instead of reaching Rust, which
// would only answer selection_in_progress. The same for the list, so Strict Mode's second effect run is free.
// Both are cleared when they settle, so nothing is cached.
let pendingList: Promise<ListResult> | null = null;
let pendingAdd: Promise<AddResult> | null = null;

export function listProjects(): Promise<ListResult> {
  if (!inDesktop()) return Promise.resolve(PREVIEW);
  pendingList ??= invoke("list_projects")
    .then(readList, readError)
    .finally(() => {
      pendingList = null;
    });
  return pendingList;
}

export function addProject(): Promise<AddResult> {
  if (!inDesktop()) return Promise.resolve(PREVIEW);
  pendingAdd ??= invoke("add_project")
    .then(readAdd, readError)
    .finally(() => {
      pendingAdd = null;
    });
  return pendingAdd;
}

const ID = /^p[1-9][0-9]{0,9}$/;

export function removeProject(id: string): Promise<RemoveResult> {
  if (!inDesktop()) return Promise.resolve(PREVIEW);
  // Rust refuses anything else with invalid_input; not sending it keeps a bad value out of the IPC.
  if (!ID.test(id)) return Promise.resolve({ kind: "error", code: "invalid_input" });
  return invoke("remove_project", { id }).then(readRemove, readError);
}

type Obj = Record<string, unknown>;

function exact(value: unknown, keys: string[]): value is Obj {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const own = Object.keys(value).sort();
  return own.length === keys.length && [...keys].sort().every((k, i) => own[i] === k);
}

// The display path is at most 1024 bytes; the name is one component of it.
const text = (v: unknown) => typeof v === "string" && v.length >= 1 && v.length <= 1024;

export function readProject(raw: unknown): Project | null {
  if (!exact(raw, ["id", "name", "path_display", "status", "registered_at_ms"])) return null;
  const { id, name, path_display, status, registered_at_ms } = raw;
  const ok =
    typeof id === "string" &&
    ID.test(id) &&
    text(name) &&
    text(path_display) &&
    STATUSES.some((s) => s === status) &&
    Number.isSafeInteger(registered_at_ms) &&
    (registered_at_ms as number) >= 0;
  return ok ? (raw as unknown as Project) : null;
}

export function readList(raw: unknown): ListResult {
  if (!exact(raw, ["schema_version", "projects"]) || raw.schema_version !== 1 || !Array.isArray(raw.projects)) return UNREADABLE;
  const projects = raw.projects.map(readProject);
  const ids = new Set(projects.map((p) => p?.id));
  // A list with an invalid or repeated record is not shown at all: half a registry would mislead.
  if (projects.some((p) => p === null) || ids.size !== projects.length) return UNREADABLE;
  return { kind: "ok", projects: projects as Project[] };
}

export function readAdd(raw: unknown): AddResult {
  if (exact(raw, ["outcome"]) && raw.outcome === "cancelled") return { kind: "cancelled" };
  if (exact(raw, ["outcome", "project"]) && (raw.outcome === "added" || raw.outcome === "already_registered")) {
    const project = readProject(raw.project);
    if (project) return { kind: raw.outcome, project };
  }
  return UNREADABLE;
}

export function readRemove(raw: unknown): RemoveResult {
  if (exact(raw, ["outcome"]) && (raw.outcome === "removed" || raw.outcome === "not_found")) return { kind: raw.outcome };
  return UNREADABLE;
}

// Rust rejects with { code, message }. Only a known code is kept; the message is never shown.
export function readError(raw: unknown): Failure {
  if (exact(raw, ["code", "message"]) && typeof raw.message === "string") {
    const code = ERROR_CODES.find((c) => c === raw.code);
    if (code) return { kind: "error", code };
  }
  return UNREADABLE;
}
