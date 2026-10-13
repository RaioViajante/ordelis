import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { findProject, MOCK_PROJECTS } from "../fixtures/mock";
import { useSampleData } from "./SampleData";

interface WorkspaceState {
  /** The selected sample project's name; null when nothing is selected or sample data is off. */
  project: string | null;
  selectProject: (name: string | null) => void;
  draft: string;
  setDraft: (text: string) => void;
  clearDraft: () => void;
  /** Ids of the selected sample context notes. Inert: nothing is read or sent anywhere. */
  contextIds: string[];
  toggleContext: (id: string) => void;
  /** Text for the polite live region. */
  announcement: { id: number; text: string };
  announce: (text: string) => void;
}

const WorkspaceContext = createContext<WorkspaceState | null>(null);
const EMPTY_CONTEXT_IDS: string[] = [];

// Session-only UI state shared by Home and Projects so the two never disagree. Nothing here is persisted.
export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const { on, revision } = useSampleData();
  const [selection, setSelection] = useState({ revision, name: MOCK_PROJECTS[0].name as string | null });
  const [draft, setDraftText] = useState("");
  const [contextSelection, setContextSelection] = useState({ revision, ids: [] as string[] });
  const [announcement, setAnnouncement] = useState({ id: 0, text: "" });
  const contextIds = contextSelection.revision === revision ? contextSelection.ids : EMPTY_CONTEXT_IDS;

  const announce = useCallback((text: string) => {
    setAnnouncement((current) => ({ id: current.id + 1, text }));
  }, []);

  const project = on && selection.revision === revision && findProject(selection.name) ? selection.name : null;

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

  const clearDraft = useCallback(() => {
    setDraftText("");
    announce("Draft cleared");
  }, [announce]);

  const toggleContext = useCallback(
    (id: string) => {
      const note = findProject(project)?.suggestions.find((s) => s.id === id);
      if (!note) return;
      const has = contextIds.includes(id);
      setContextSelection({ revision, ids: has ? contextIds.filter((c) => c !== id) : [...contextIds, id] });
      announce(`${note.title} ${has ? "removed from" : "added to"} sample context`);
    },
    [project, contextIds, revision, announce],
  );

  const value = useMemo(
    () => ({ project, selectProject, draft, setDraft: setDraftText, clearDraft, contextIds, toggleContext, announcement, announce }),
    [project, selectProject, draft, clearDraft, contextIds, toggleContext, announcement, announce],
  );
  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>;
}

export function useWorkspace(): WorkspaceState {
  const ctx = useContext(WorkspaceContext);
  if (!ctx) throw new Error("useWorkspace must be used inside WorkspaceProvider");
  return ctx;
}
