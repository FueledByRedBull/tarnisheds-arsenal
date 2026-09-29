import { useCallback, useEffect, useState } from "react";

export function useKeyedResource<T>(key: string | null, load: (signal: AbortSignal) => Promise<T>) {
  const [attempt, setAttempt] = useState(0);
  const [loaded, setLoaded] = useState<{
    key: string;
    attempt: number;
    data: T | null;
    error: string | null;
  } | null>(null);
  const retry = useCallback(() => setAttempt(current => current + 1), []);

  useEffect(() => {
    if (key === null) return;
    const controller = new AbortController();
    load(controller.signal).then(data => {
      if (!controller.signal.aborted) setLoaded({ key, attempt, data, error: null });
    }).catch(error => {
      if (!controller.signal.aborted) setLoaded({
        key, attempt, data: null, error: error instanceof Error ? error.message : String(error),
      });
    });
    return () => controller.abort();
  }, [key, load, attempt]);

  const current = loaded?.key === key && loaded.attempt === attempt ? loaded : null;
  return {
    data: current?.data ?? null,
    error: current?.error ?? null,
    status: key === null ? "idle" as const : !current ? "loading" as const
      : current.error !== null ? "error" as const : "ready" as const,
    retry,
  };
}
