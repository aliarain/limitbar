import { cardState, formatPercent, formatReset, minRemaining, statusCopy } from "../lib/format";
import type { ProviderView, UsageWindow } from "../types/usage";

interface Props {
  view: ProviderView;
  now: number;
}

function windowLine(w: UsageWindow, now: number): string {
  const pct = formatPercent(w.remaining_percent);
  const reset = formatReset(w.reset_at, now);
  if (reset) return `${pct} · ${reset.replace(/^Resets /, "resets ")}`;
  if (w.remaining_percent === 100) return `${pct} · idle`;
  return pct;
}

export function ProviderCard({ view, now }: Props) {
  const state = cardState(view);
  const snap = view.snapshot;
  const windows = snap?.windows ?? [];
  const remaining = minRemaining(view);
  const note = statusCopy(view.status, view.freshness, view.last_error);
  const binding = windows.reduce<UsageWindow | null>(
    (acc, w) => (w.remaining_percent !== null && (acc === null || w.remaining_percent < (acc.remaining_percent ?? 101)) ? w : acc),
    null,
  );
  const subtitle = [snap?.plan_name, snap?.account_identifier ? `@${snap.account_identifier}` : null].filter(Boolean).join(" · ");

  return (
    <section className={`row row--${state}`} data-state={state} aria-label={view.provider_name}>
      <div className="row__top">
        <div className="row__title">
          <span className="row__name">{view.provider_name}</span>
          {subtitle && <span className="row__sub">{subtitle}</span>}
        </div>
        <span className="row__pct">{state === "loading" ? "…" : formatPercent(remaining)}</span>
      </div>

      {binding && binding.remaining_percent !== null && (
        <div className="bar" role="progressbar" aria-valuenow={Math.round(binding.remaining_percent)} aria-valuemin={0} aria-valuemax={100}>
          <div className="bar__fill" style={{ width: `${Math.max(0, Math.min(100, binding.remaining_percent))}%` }} />
        </div>
      )}

      <div className="row__meta">
        {windows.length > 0 ? (
          windows.map((w) => (
            <span key={w.id} className={`win${w.exceeded ? " win--exceeded" : ""}${w === binding ? " win--binding" : ""}`}>
              <span className="win__label">{w.label}</span>
              {windowLine(w, now)}
            </span>
          ))
        ) : state === "loading" ? (
          <span>Loading…</span>
        ) : (
          <span>{snap?.message ?? "Usage unavailable"}</span>
        )}
      </div>

      {snap?.detail && <div className="row__detail">{snap.detail}</div>}
      {note && <div className={`row__note row__note--${view.status.toLowerCase()}`}>{note}</div>}
    </section>
  );
}
