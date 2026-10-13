import { useCallback, useEffect, useRef, useState } from "react";
import { inDesktop, queryRuntimeStatus, type RuntimeResult } from "./runtimeStatus";

export interface RuntimeView {
  /** False in a browser preview, where no native query is ever attempted. */
  desktop: boolean;
  loading: boolean;
  /** The last answer, kept on screen while a refresh is in flight. Held in memory only. */
  result: RuntimeResult | null;
}

// Queries once when the component mounts and again only on refresh: no polling, no automatic retry.
// Strict Mode's second effect run shares the first request through queryRuntimeStatus, so it reaches Rust once.
export function useRuntimeStatus(): RuntimeView & { refresh: () => void } {
  const [desktop] = useState(inDesktop);
  const [view, setView] = useState<Omit<RuntimeView, "desktop">>({ loading: desktop, result: null });
  const mounted = useRef(false);

  const run = useCallback(() => {
    queryRuntimeStatus().then((result) => {
      if (mounted.current) setView({ loading: false, result });
    });
  }, []);

  useEffect(() => {
    mounted.current = true;
    if (desktop) run();
    return () => {
      mounted.current = false;
    };
  }, [desktop, run]);

  const refresh = useCallback(() => {
    if (!desktop || view.loading) return;
    setView((v) => ({ ...v, loading: true }));
    run();
  }, [desktop, view.loading, run]);

  return { desktop, ...view, refresh };
}
