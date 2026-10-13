import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { entriesFor, findProject, isSelectable, MOCK_PROJECTS, type ContextEntry } from "../fixtures/mock";
import { findSession } from "../fixtures/sessions";
import { useSampleData } from "./SampleData";

interface WorkspaceState {
  /** The selected sample project's name; null when nothing is selected or sample data is off. */
  project: string | null;
  selectProject: (name: string | null) => void;
  /** The selected sample session's id; null when nothing is selected or sample data is off. */
  session: string | null;
  selectSession: (id: string | null) => void;
  draft: string;
  setDraft: (text: string) => void;
  clearDraft: () => void;
  /** Sample entries that can be selected now: the project's notes and history, minus anything unavailable. */
  selectable: ContextEntry[];
  /** Ids of the selected sample context entries. Inert: nothing is read or sent anywhere. */
  contextIds: string[];
  /** The selected entries, resolved against the current project, sample mode and history setting. */
  contextEntries: ContextEntry[];
  toggleContext: (id: string) => void;
  clearContext: () => void;
  /** Whether sample private history may be offered (and so selected) as context. */
  historyOffered: boolean;
  setHistoryOffered: (offered: boolean) => void;
  /** Text for the polite live region. */
  announcement: { id: number; text: string };
  announce: (text: string) => void;
}

const WorkspaceContext = createContext<WorkspaceState | null>(null);
const EMPTY_CONTEXT_IDS: string[] = [];
const EMPTY_ENTRIES: ContextEntry[] = [];

// Session-only UI state shared by Home and Projects so the two never disagree. Nothing here is persisted.
export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const { on, revision } = useSampleData();
  const [selection, setSelection] = useState({ revision, name: MOCK_PROJECTS[0].name as string | null });
  const [sessionSel, setSessionSel] = useState({ revision, id: "b71c" as string | null });
  const [draft, setDraftText] = useState("");
  const [contextSelection, setContextSelection] = useState({ revision, ids: [] as string[] });
  const [historyOffered, setHistoryOfferedState] = useState(true);
  const [announcement, setAnnouncement] = useState({ id: 0, text: "" });
  const storedIds = contextSelection.revision === revision ? contextSelection.ids : EMPTY_CONTEXT_IDS;

  const announce = useCallback((text: string) => {
    setAnnouncement((current) => ({ id: current.id + 1, text }));
  }, []);

  const project = on && selection.revision === revision && findProject(selection.name) ? selection.name : null;

  const session = on && sessionSel.revision === revision && findSession(sessionSel.id) ? sessionSel.id : null;

  // The selection is always resolved against what is selectable now, so a stale id cannot surface.
  const selectable = useMemo(
    () => (on ? entriesFor(project).filter((e) => isSelectable(e) && (historyOffered || e.source !== "history")) : EMPTY_ENTRIES),
    [on, project, historyOffered],
  );
  const contextEntries = useMemo(() => selectable.filter((e) => storedIds.includes(e.id)), [selectable, storedIds]);
  const contextIds = useMemo(() => contextEntries.map((e) => e.id), [contextEntries]);

  const selectProject = useCallback(
    (name: string | null) => {
      if (name === project) return;
      setSelection({ revision, name });
      // Context notes belong to a project, so they do not carry over.
      setContextSelection({ revision, ids: [] });
      announce(name ? `Project ${name} selected` : "No project selected");
    },
    [project, revision, announce],
  );

  const selectSession = useCallback(
    (id: string | null) => {
      if (id === session) return;
      setSessionSel({ revision, id });
      announce(id ? `Session ${id} selected` : "No session selected");
    },
    [session, revision, announce],
  );

  const clearDraft = useCallback(() => {
    setDraftText("");
    announce("Draft cleared");
  }, [announce]);

  const toggleContext = useCallback(
    (id: string) => {
      const entry = selectable.find((e) => e.id === id);
      if (!entry) return;
      const has = contextIds.includes(id);
      setContextSelection({ revision, ids: has ? contextIds.filter((c) => c !== id) : [...contextIds, id] });
      announce(`${entry.title} ${has ? "removed from" : "added to"} sample context`);
    },
    [selectable, contextIds, revision, announce],
  );

  const clearContext = useCallback(() => {
    setContextSelection({ revision, ids: [] });
    announce("Sample context selection cleared");
  }, [revision, announce]);

  const setHistoryOffered = useCallback(
    (offered: boolean) => {
      setHistoryOfferedState(offered);
      if (!offered) {
        // Dropped explicitly, so turning the setting back on does not resurrect old choices.
        setContextSelection({ revision, ids: contextEntries.filter((e) => e.source !== "history").map((e) => e.id) });
      }
      announce(offered ? "Sample past decisions offered as context" : "Sample past decisions no longer offered");
    },
    [revision, contextEntries, announce],
  );

  const value = useMemo(
    () => ({
      project, selectProject, session, selectSession, draft, setDraft: setDraftText, clearDraft, selectable, contextIds, contextEntries,
      toggleContext, clearContext, historyOffered, setHistoryOffered, announcement, announce,
    }),
    [project, selectProject, session, selectSession, draft, clearDraft, selectable, contextIds, contextEntries, toggleContext, clearContext, historyOffered, setHistoryOffered, announcement, announce],
  );
  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>;
}

export function useWorkspace(): WorkspaceState {
  const ctx = useContext(WorkspaceContext);
  if (!ctx) throw new Error("useWorkspace must be used inside WorkspaceProvider");
  return ctx;
}
