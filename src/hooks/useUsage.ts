import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import type { ProviderView } from "../types/usage";

const USAGE_EVENT = "usage://changed";

export function useUsage() {
  const [views, setViews] = useState<ProviderView[]>([]);
  const [refreshing, setRefreshing] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    invoke<ProviderView[]>("get_usage")
      .then((v) => { if (!cancelled) setViews(v); })
      .catch((e) => console.error("get_usage failed", e));
    listen<ProviderView[]>(USAGE_EVENT, (e) => setViews(e.payload))
      .then((u) => { if (cancelled) u(); else unlisten = u; })
      .catch((e) => console.error("listen failed", e));
    return () => { cancelled = true; unlisten?.(); };
  }, []);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      setViews(await invoke<ProviderView[]>("refresh_usage"));
    } catch (e) {
      console.error("refresh_usage failed", e);
    } finally {
      setRefreshing(false);
    }
  }, []);

  return { views, refresh, refreshing };
}
