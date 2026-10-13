import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";

interface SampleDataState {
  on: boolean;
  revision: number;
  setOn: (on: boolean) => void;
}

const SampleDataContext = createContext<SampleDataState | null>(null);

// Sample data is on by default so the screens can be reviewed against the
// reference; it is always labelled and can be switched off from the banner or
// Settings. It is in-memory only.
export function SampleDataProvider({ children, initial = true }: { children: ReactNode; initial?: boolean }) {
  const [state, setState] = useState({ on: initial, revision: 0 });
  const setOn = useCallback((on: boolean) => {
    setState((current) =>
      current.on === on ? current : { on, revision: on ? current.revision : current.revision + 1 },
    );
  }, []);
  const value = useMemo(() => ({ ...state, setOn }), [state, setOn]);
  return <SampleDataContext.Provider value={value}>{children}</SampleDataContext.Provider>;
}

export function useSampleData(): SampleDataState {
  const ctx = useContext(SampleDataContext);
  if (!ctx) throw new Error("useSampleData must be used inside SampleDataProvider");
  return ctx;
}
