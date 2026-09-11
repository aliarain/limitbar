import { invoke } from "@tauri-apps/api/core";
import { formatAge, latestFetch } from "../lib/format";
import type { ProviderView } from "../types/usage";
import { ProviderCard } from "./ProviderCard";

interface Props {
  views: ProviderView[];
  now: number;
  refreshing: boolean;
  onRefresh: () => void;
}

/** Replaces the chip on hover, anchored to the same corner. Header is draggable. */
export function DetailPanel({ views, now, refreshing, onRefresh }: Props) {
  const updated = formatAge(latestFetch(views), now);
  return (
    <aside className="panel" aria-label="Details">
      <header className="panel__head" data-tauri-drag-region>
        <span className="panel__title" data-tauri-drag-region>LimitBar</span>
        <div className="panel__actions">
          <button type="button" className="icon-btn" onClick={onRefresh} disabled={refreshing} title="Refresh now" aria-label="Refresh">
            <svg className={refreshing ? "spin" : undefined} width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
              <path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9" /><path d="M13.5 2.5v3h-3" />
            </svg>
          </button>
          <button type="button" className="icon-btn" onClick={() => invoke("hide_popup")} title="Hide widget (reopen from the menu bar)" aria-label="Hide">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"><path d="M4 4l8 8M12 4l-8 8" /></svg>
          </button>
          <button type="button" className="icon-btn" onClick={() => invoke("quit_app")} title="Quit LimitBar" aria-label="Quit">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"><path d="M8 2.5v5.5" /><path d="M4.6 4.6a5 5 0 1 0 6.8 0" /></svg>
          </button>
        </div>
      </header>
      <div className="panel__body">
        {views.map((v) => <ProviderCard key={v.provider_id} view={v} now={now} />)}
      </div>
      <footer className="panel__foot">{refreshing ? "Refreshing…" : (updated ?? "Not updated yet")}</footer>
    </aside>
  );
}
