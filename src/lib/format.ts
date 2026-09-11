import type { Freshness, ProviderView, UsageStatus, UsageWindow } from "../types/usage";

/** Visual state of a provider card. Drives colour and copy. */
export type CardState =
  | "normal"
  | "low"
  | "critical"
  | "loading"
  | "stale"
  | "unavailable"
  | "auth_required";

export const LOW_THRESHOLD = 20;
export const CRITICAL_THRESHOLD = 10;

export function cardState(v: ProviderView): CardState {
  const remaining = minRemaining(v);
  if (v.status === "AUTH_REQUIRED") return "auth_required";
  if (v.snapshot === null) {
    return v.freshness === "REFRESHING" || v.freshness === "NEVER" ? "loading" : "unavailable";
  }
  if (remaining === null) return "unavailable";
  if (v.freshness === "STALE") return "stale";
  if (remaining <= CRITICAL_THRESHOLD) return "critical";
  if (remaining <= LOW_THRESHOLD) return "low";
  return "normal";
}

/** Lowest remaining % across the provider's windows — the binding constraint. */
export function minRemaining(v: ProviderView): number | null {
  const values = (v.snapshot?.windows ?? [])
    .map((w) => w.remaining_percent)
    .filter((p): p is number => typeof p === "number" && Number.isFinite(p));
  return values.length ? Math.min(...values) : null;
}

/** "72%" — never "NaN%", never a fabricated number. */
export function formatPercent(p: number | null): string {
  if (p === null || !Number.isFinite(p)) return "—";
  return `${Math.round(Math.min(100, Math.max(0, p)))}%`;
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/**
 * Renders an absolute reset instant relative to `now` (both epoch ms):
 *   Resets in 47m · Resets in 2h 14m · Resets today at 8:00 PM ·
 *   Resets tomorrow at 5:00 AM · Resets Sep 14
 */
export function formatReset(resetAtIso: string | null, now: number = Date.now()): string | null {
  if (!resetAtIso) return null;
  const t = Date.parse(resetAtIso);
  if (!Number.isFinite(t)) return null;
  const diff = t - now;
  if (diff <= 0) return "Resetting…";
  if (diff < HOUR) return `Resets in ${Math.max(1, Math.round(diff / MINUTE))}m`;
  if (diff < 6 * HOUR) {
    const h = Math.floor(diff / HOUR);
    const m = Math.round((diff - h * HOUR) / MINUTE);
    return m === 0 ? `Resets in ${h}h` : `Resets in ${h}h ${m}m`;
  }
  const target = new Date(t);
  const today = new Date(now);
  const time = target.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  const sameDay = target.toDateString() === today.toDateString();
  if (sameDay) return `Resets today at ${time}`;
  const tomorrow = new Date(now + DAY);
  if (target.toDateString() === tomorrow.toDateString()) return `Resets tomorrow at ${time}`;
  return `Resets ${target.toLocaleDateString(undefined, { month: "short", day: "numeric" })}`;
}

/** "Updated 12 sec ago" / "Updated 18 min ago" / "Updated 3 h ago". */
export function formatAge(iso: string | null, now: number = Date.now()): string | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return null;
  const s = Math.max(0, Math.round((now - t) / 1000));
  if (s < 60) return `Updated ${s} sec ago`;
  if (s < 3600) return `Updated ${Math.round(s / 60)} min ago`;
  if (s < DAY / 1000) return `Updated ${Math.round(s / 3600)} h ago`;
  return `Updated ${Math.round(s / (DAY / 1000))} d ago`;
}

/** One-line explanation for non-normal statuses. Never includes secrets. */
export function statusCopy(status: UsageStatus, freshness: Freshness, lastError: string | null): string | null {
  switch (status) {
    case "AUTH_REQUIRED":
      return "Sign in required";
    case "RATE_LIMITED":
      return "Provider is rate limiting — retrying later";
    case "ERROR":
      return lastError ? `Refresh failed: ${lastError}` : "Refresh failed";
    case "UNSUPPORTED":
      return "Not supported";
    case "UNAVAILABLE":
      return freshness === "NEVER" || freshness === "REFRESHING" ? null : "Usage unavailable";
    default:
      return null;
  }
}

/** Latest fetched_at across providers, for the footer. */
export function latestFetch(views: ProviderView[]): string | null {
  const ts = views
    .map((v) => v.snapshot?.fetched_at ?? null)
    .filter((s): s is string => s !== null)
    .map((s) => Date.parse(s))
    .filter(Number.isFinite);
  return ts.length ? new Date(Math.max(...ts)).toISOString() : null;
}

export function windowSummary(w: UsageWindow, now: number = Date.now()): string {
  const reset = formatReset(w.reset_at, now);
  const pct = formatPercent(w.remaining_percent);
  return reset ? `${w.label} ${pct} · ${reset.replace(/^Resets /, "resets ")}` : `${w.label} ${pct}`;
}
