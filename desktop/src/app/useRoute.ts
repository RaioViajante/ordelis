import { useCallback, useEffect, useState } from "react";
import { hrefFor, parseHash, type RouteId } from "./routes";

export function useRoute(): [RouteId, (id: RouteId) => void] {
  const [route, setRoute] = useState<RouteId>(() => parseHash(window.location.hash));

  useEffect(() => {
    const onChange = () => setRoute(parseHash(window.location.hash));
    window.addEventListener("hashchange", onChange);
    return () => window.removeEventListener("hashchange", onChange);
  }, []);

  const go = useCallback((id: RouteId) => {
    window.location.hash = hrefFor(id);
  }, []);

  return [route, go];
}
