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
  announcement: string;
  announce: (text: string) => void;
}

const WorkspaceContext = createContext<WorkspaceState | null>(null);

// Session-only UI state shared by Home and Projects so the two never disagree. Nothing here is persisted.
export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const { on } = useSampleData();
  const [selected, setSelected] = useState<string | null>(MOCK_PROJECTS[0].name);
  const [draft, setDraftText] = useState("");
  const [contextIds, setContextIds] = useState<string[]>([]);
  const [announcement, announce] = useState("");

  const project = on && findProject(selected) ? selected : null;

  const selectProject = useCallback(
    (name: string | null) => {
      if (name === selected) return;
      setSelected(name);
      // Context notes belong to a project, so they do not carry over.
      setContextIds([]);
      announce(name ? `Project ${name} selected` : "No project selected");
    },
    [selected],
  );

  const clearDraft = useCallback(() => {
    setDraftText("");
    announce("Draft cleared");
  }, []);

  const toggleContext = useCallback(
    (id: string) => {
      const note = findProject(project)?.suggestions.find((s) => s.id === id);
      if (!note) return;
      const has = contextIds.includes(id);
      setContextIds(has ? contextIds.filter((c) => c !== id) : [...contextIds, id]);
      announce(`${note.title} ${has ? "removed from" : "added to"} sample context`);
    },
    [project, contextIds],
  );

  const value = useMemo(
    () => ({ project, selectProject, draft, setDraft: setDraftText, clearDraft, contextIds, toggleContext, announcement, announce }),
    [project, selectProject, draft, clearDraft, contextIds, toggleContext, announcement],
  );
  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>;
}

export function useWorkspace(): WorkspaceState {
  const ctx = useContext(WorkspaceContext);
  if (!ctx) throw new Error("useWorkspace must be used inside WorkspaceProvider");
  return ctx;
}
