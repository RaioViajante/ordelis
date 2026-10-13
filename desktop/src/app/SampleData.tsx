import { createContext, useContext, useMemo, useState, type ReactNode } from "react";

interface SampleDataState {
  on: boolean;
  setOn: (on: boolean) => void;
}

const SampleDataContext = createContext<SampleDataState | null>(null);

// Sample data is on by default so the screens can be reviewed against the
// reference; it is always labelled and can be switched off from the banner or
// Settings. It is in-memory only.
export function SampleDataProvider({ children, initial = true }: { children: ReactNode; initial?: boolean }) {
  const [on, setOn] = useState(initial);
  const value = useMemo(() => ({ on, setOn }), [on]);
  return <SampleDataContext.Provider value={value}>{children}</SampleDataContext.Provider>;
}

export function useSampleData(): SampleDataState {
  const ctx = useContext(SampleDataContext);
  if (!ctx) throw new Error("useSampleData must be used inside SampleDataProvider");
  return ctx;
}
