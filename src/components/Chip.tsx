import { cardState, formatPercent, minRemaining } from "../lib/format";
import { SHORT_NAME } from "../lib/names";
import { ProviderGlyph } from "./ProviderGlyph";
import type { ProviderView } from "../types/usage";

interface Props {
  views: ProviderView[];
}

/** The always-visible part: one line per provider — name, bar, percent. */
export function Chip({ views }: Props) {
  return (
    <div className="chip" data-tauri-drag-region>
      {views.map((v) => {
        const state = cardState(v);
        const remaining = minRemaining(v);
        return (
          <div key={v.provider_id} className={`chip__row chip__row--${state}`} data-state={state} data-tauri-drag-region>
            <span className="chip__glyph" data-tauri-drag-region><ProviderGlyph id={v.provider_id} size={11} /></span>
            <span className="chip__name" data-tauri-drag-region>{SHORT_NAME[v.provider_id] ?? v.provider_name}</span>
            <span className="chip__bar" data-tauri-drag-region>
              {remaining !== null && <span className="chip__fill" style={{ width: `${Math.max(0, Math.min(100, remaining))}%` }} />}
            </span>
            <span className="chip__pct" data-tauri-drag-region>
              {state === "loading" ? "…" : state === "auth_required" ? "⚠" : formatPercent(remaining)}
            </span>
          </div>
        );
      })}
      {views.length === 0 && <div className="chip__row"><span className="chip__name">LimitBar</span><span className="chip__pct">…</span></div>}
    </div>
  );
}
