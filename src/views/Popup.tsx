import { invoke } from "@tauri-apps/api/core";
import { useRef } from "react";
import { ProviderCard } from "../components/ProviderCard";
import { useFitWindow } from "../hooks/useFitWindow";
import { useNow } from "../hooks/useNow";
import { useUsage } from "../hooks/useUsage";
import { formatAge, latestFetch } from "../lib/format";

export function Popup() {
  const { views, refresh, refreshing } = useUsage();
  const now = useNow(15_000);
  const root = useRef<HTMLDivElement>(null);
  const anyRefreshing = refreshing || views.some((v) => v.freshness === "REFRESHING");
  const updated = formatAge(latestFetch(views), now);
  const enabled = views.filter((v) => v.enabled);

  useFitWindow(root, [views, updated]);

  return (
    <div className="popup" ref={root}>
      <header className="popup__head" data-tauri-drag-region>
        <span className="popup__title">LimitBar</span>
        <div className="popup__actions">
          <button type="button" className="icon-btn" onClick={refresh} disabled={anyRefreshing} title="Refresh now" aria-label="Refresh">
            <svg className={anyRefreshing ? "spin" : undefined} width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
              <path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9" />
              <path d="M13.5 2.5v3h-3" />
            </svg>
          </button>
          <button type="button" className="icon-btn" onClick={() => invoke("quit_app")} title="Quit LimitBar" aria-label="Quit">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round">
              <path d="M8 2.5v5.5" />
              <path d="M4.6 4.6a5 5 0 1 0 6.8 0" />
            </svg>
          </button>
        </div>
      </header>

      <main className="popup__body">
        {enabled.map((v) => (
          <ProviderCard key={v.provider_id} view={v} now={now} />
        ))}
        {views.length === 0 && <p className="popup__empty">Starting…</p>}
      </main>

      <footer className="popup__foot">{anyRefreshing ? "Refreshing…" : (updated ?? "Not updated yet")}</footer>
    </div>
  );
}
