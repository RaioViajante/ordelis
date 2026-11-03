import { createContext, useCallback, useContext, useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import {
  addProject,
  inDesktop,
  listProjects,
  removeProject,
  type AddResult,
  type Project,
  type ProjectErrorCode,
  type RemoveResult,
} from "./projects";
import { useWorkspace } from "./Workspace";

/** Why the list could not be read. "unreadable" is an answer outside the Rust contract. */
export type ListError = ProjectErrorCode | "unreadable";

export interface Message {
  tone: "ok" | "warn" | "error";
  text: string;
}

interface RegistryState {
  /** False in a browser preview, where no native call is ever attempted. */
  desktop: boolean;
  /** The first list is in flight. */
  loading: boolean;
  /** The last list Rust answered, in registration order. Held in memory; Rust owns the persistent copy. */
  projects: Project[];
  /** The last list failed. Adding and removing wait until a list succeeds. */
  listError: ListError | null;
  /** The native folder panel is open (or its answer is being read). */
  adding: boolean;
  /** The id being removed. */
  removing: string | null;
  add: () => Promise<AddResult>;
  remove: (id: string) => Promise<RemoveResult>;
  reload: () => Promise<void>;
}

const RegistryContext = createContext<RegistryState | null>(null);

// The UI words every code itself, so no native text is shown and the product name follows the interface.
const ERROR_TEXT: Record<ListError, string> = {
  not_allowed: "Folders can only be added from the main window.",
  selection_in_progress: "A folder selection is already open. Finish or cancel it, then try again.",
  not_a_directory: "That item is not a folder.",
  folder_refused: "That folder is too broad to register as a project. Choose the folder of one project.",
  folder_changed: "The folder changed while it was being added. Nothing was saved. Try again.",
  folder_inaccessible: "That folder could not be accessed. Nothing was saved.",
  path_unsupported: "That folder's location is not supported.",
  registry_full: "The project list is full. Remove a project to add another.",
  invalid_input: "That project could not be found.",
  registry_corrupt: "The project list could not be read. Its file was kept unchanged and nothing was deleted.",
  registry_newer_version: "The project list was saved by a newer version of Ordelis. Update Ordelis to use it.",
  registry_unavailable: "The project list is unavailable.",
  unsupported_platform: "Local projects are only supported on macOS for now.",
  unreadable: "Ordelis could not read the answer for the project list.",
};

export const errorText = (code: ListError) => ERROR_TEXT[code];

const STATUS_NOTE: Record<Project["status"], string> = {
  available: "",
  missing: " Its folder was not found at the saved location.",
  inaccessible: " Its folder cannot be accessed.",
  changed: " A different folder is now at that location.",
};

export function describeAdd(result: AddResult): Message | null {
  switch (result.kind) {
    case "added":
      return { tone: "ok", text: `Added ${result.project.name}. Ordelis saved only its location and has not read its contents.` };
    case "already_registered":
      return {
        tone: result.project.status === "available" ? "ok" : "warn",
        text: `${result.project.name} is already in your projects.${STATUS_NOTE[result.project.status]}`,
      };
    case "cancelled":
    case "preview":
      return null;
    case "error":
      return { tone: result.code === "selection_in_progress" ? "warn" : "error", text: errorText(result.code) };
    case "unreadable":
      return { tone: "error", text: errorText("unreadable") };
  }
}

export function describeRemove(result: RemoveResult, name: string): Message | null {
  switch (result.kind) {
    case "removed":
      return { tone: "ok", text: `Removed ${name} from Ordelis. The folder and its files were not touched.` };
    case "not_found":
      return { tone: "warn", text: `${name} was already removed.` };
    case "preview":
      return null;
    case "error":
      return { tone: "error", text: errorText(result.code) };
    case "unreadable":
      return { tone: "error", text: errorText("unreadable") };
  }
}

// Home, Projects and the sidebar read the same state, so a change made in one is seen by all of them. Rust owns
// persistence: nothing here is written anywhere, and the list is read again after every change.
export function RegistryProvider({ children }: { children: ReactNode }) {
  const { announce } = useWorkspace();
  const [desktop] = useState(inDesktop);
  const [list, setList] = useState<{ loading: boolean; projects: Project[]; error: ListError | null }>({
    loading: desktop,
    projects: [],
    error: null,
  });
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);
  const mounted = useRef(false);
  const busy = useRef(false);

  const reload = useCallback(
    () =>
      listProjects().then((result) => {
        if (!mounted.current) return;
        if (result.kind === "ok") setList({ loading: false, projects: result.projects, error: null });
        else if (result.kind === "preview") setList({ loading: false, projects: [], error: null });
        else setList((l) => ({ loading: false, projects: l.projects, error: result.kind === "error" ? result.code : "unreadable" }));
      }),
    [],
  );

  useEffect(() => {
    mounted.current = true;
    if (desktop) reload();
    return () => {
      mounted.current = false;
    };
  }, [desktop, reload]);

  const add = useCallback(async (): Promise<AddResult> => {
    // A second request while one is open never reaches Rust.
    if (busy.current) return { kind: "error", code: "selection_in_progress" };
    busy.current = true;
    setAdding(true);
    try {
      const result = await addProject();
      // After a failure too: the answer may be stale (a page reload leaves a panel open), so the list is read again.
      if (result.kind !== "cancelled" && result.kind !== "preview") await reload();
      const message = describeAdd(result);
      if (message) announce(message.text);
      return result;
    } finally {
      busy.current = false;
      if (mounted.current) setAdding(false);
    }
  }, [reload, announce]);

  const remove = useCallback(
    async (id: string): Promise<RemoveResult> => {
      if (busy.current) return { kind: "error", code: "selection_in_progress" };
      busy.current = true;
      setRemoving(id);
      try {
        const name = list.projects.find((p) => p.id === id)?.name ?? "The project";
        const result = await removeProject(id);
        if (result.kind !== "preview") await reload();
        const message = describeRemove(result, name);
        if (message) announce(message.text);
        return result;
      } finally {
        busy.current = false;
        if (mounted.current) setRemoving(null);
      }
    },
    [list.projects, reload, announce],
  );

  const value = useMemo(
    () => ({ desktop, loading: list.loading, projects: list.projects, listError: list.error, adding, removing, add, remove, reload }),
    [desktop, list, adding, removing, add, remove, reload],
  );
  return <RegistryContext.Provider value={value}>{children}</RegistryContext.Provider>;
}

/**
 * The add-a-folder action for a button: whether it is available and why not, the result message to show, and the
 * action itself. `onAdded` runs for a new or an already registered project.
 */
export function useAddFolder(onAdded?: (project: Project) => void) {
  const reg = useRegistry();
  const [message, setMessage] = useState<Message | null>(null);
  const reason = !reg.desktop
    ? "Adding folders needs the desktop app."
    : reg.loading
      ? "Loading your projects…"
      : reg.listError
        ? "The project list needs attention first."
        : reg.adding
          ? "A folder selection is already open."
          : undefined;
  // The button is rebuilt while the panel is open (it carries a reason then), so it would lose keyboard focus.
  // When this button started the selection, focus returns to it once the panel is closed.
  const id = useId();
  const started = useRef(false);
  const { adding } = reg;
  useEffect(() => {
    if (!adding && started.current) {
      started.current = false;
      document.getElementById(id)?.focus();
    }
  }, [adding, id]);
  const add = async () => {
    setMessage(null);
    started.current = true;
    const result = await reg.add();
    setMessage(describeAdd(result));
    if (result.kind === "added" || result.kind === "already_registered") onAdded?.(result.project);
  };
  return { message, setMessage, reason, add, adding, id };
}

export function useRegistry(): RegistryState {
  const ctx = useContext(RegistryContext);
  if (!ctx) throw new Error("useRegistry must be used inside RegistryProvider");
  return ctx;
}
