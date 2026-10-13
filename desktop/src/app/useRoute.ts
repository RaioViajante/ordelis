import { useCallback, useEffect, useState } from "react";
import { hrefFor, parseHash, type Location } from "./routes";

export function useRoute(): [Location, (page: Location["page"], param?: string) => void] {
  const [loc, setLoc] = useState<Location>(() => parseHash(window.location.hash));

  useEffect(() => {
    const onChange = () => setLoc(parseHash(window.location.hash));
    window.addEventListener("hashchange", onChange);
    return () => window.removeEventListener("hashchange", onChange);
  }, []);

  const go = useCallback((page: Location["page"], param?: string) => {
    window.location.hash = hrefFor(page, param);
  }, []);

  return [loc, go];
}
