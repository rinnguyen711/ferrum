import { useCallback, useEffect, useState } from "react";
import { ApiError } from "../api/client";

export interface Resource<T> {
  data: T | null;
  loading: boolean;
  error: ApiError | null;
  refetch: () => void;
}

/**
 * Run `fetcher` whenever `deps` change. Stale loads are ignored via a
 * per-run flag. 401s never land here — the client intercepts them.
 */
export function useResource<T>(fetcher: () => Promise<T>, deps: unknown[]): Resource<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<ApiError | null>(null);
  const [nonce, setNonce] = useState(0);

  const refetch = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    let ignore = false;
    setLoading(true);
    setError(null);
    fetcher()
      .then((d) => {
        if (!ignore) setData(d);
      })
      .catch((e) => {
        if (ignore) return;
        if (e instanceof Error && e.name === "AuthError") return; // handled globally
        setError(e instanceof ApiError ? e : new ApiError(0, "error", String(e)));
      })
      .finally(() => {
        if (!ignore) setLoading(false);
      });
    return () => {
      ignore = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, nonce]);

  return { data, loading, error, refetch };
}
