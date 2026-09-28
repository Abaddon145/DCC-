import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import type { ViewMode } from "../types";
import { api } from "./api";

export const isAssetWorkspace = (view: ViewMode) => ["library", "favorites", "recent", "health"].includes(view);

export class RequestGate {
  private version = 0;
  begin() { return ++this.version; }
  invalidate() { ++this.version; }
  isCurrent(version: number) { return this.version === version; }
}

export function useRequestGate() {
  const gate = useRef(new RequestGate()).current;
  useEffect(() => () => gate.invalidate(), [gate]);
  return gate;
}

/** Each context is written separately and bound to a library UUID, never a whole stale preferences object. */
export function useWorkspaceState<T extends object>(moduleId: string, initial: T, revision = 0): [T, Dispatch<SetStateAction<T>>, boolean] {
  const [value, setValue] = useState(initial);
  const initialRef = useRef(initial); initialRef.current = initial;
  const current = useRef(value); current.current = value;
  const location = useRef<{ libraryId: string; enabled: boolean } | null>(null);
  const changed = useRef(false);
  const [loaded, setLoaded] = useState(false);
  const set: Dispatch<SetStateAction<T>> = useCallback(next => { changed.current = true; setValue(next); }, []);
  useEffect(() => {
    let active = true; changed.current = false; location.current = null; setLoaded(false); setValue(initialRef.current);
    const defaults = initialRef.current;
    if (!api.getWorkspaceContext) setLoaded(true);
    else void api.getWorkspaceContext(moduleId).then(result => {
      if (!active) return;
      location.current = result;
      if (result.enabled && result.value && !changed.current) setValue({ ...defaults, ...(result.value as Partial<T>) });
      setLoaded(true);
    }).catch(() => { if (active) setLoaded(true); });
    return () => {
      active = false;
      const target = location.current;
      if (target?.enabled) void api.saveWorkspaceContext(target.libraryId, moduleId, current.current).catch(() => undefined);
    };
  }, [moduleId, revision]);
  useEffect(() => {
    const target = location.current;
    if (!loaded || !target?.enabled) return;
    const timer = window.setTimeout(() => void api.saveWorkspaceContext(target.libraryId, moduleId, value).catch(() => undefined), 500);
    return () => window.clearTimeout(timer);
  }, [moduleId, revision, value, loaded]);
  return [value, set, loaded];
}
