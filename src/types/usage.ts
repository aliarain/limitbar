// Mirrors src-tauri/src/usage/{models,manager}.rs. Keep in sync by hand; the
// Rust side is the source of truth.

export type ProviderId = "claude" | "codex" | "command-code" | "gemini";

export type UsageStatus =
  | "AVAILABLE"
  | "UNAVAILABLE"
  | "AUTH_REQUIRED"
  | "RATE_LIMITED"
  | "ERROR"
  | "UNSUPPORTED";

export type UsageSource = "OFFICIAL_API" | "LOCAL_PROVIDER_DATA" | "LOCAL_CLI" | "ESTIMATED";

export type Freshness = "NEVER" | "FRESH" | "REFRESHING" | "STALE";

export interface UsageWindow {
  id: string;
  label: string;
  used_percent: number | null;
  remaining_percent: number | null;
  /** RFC 3339 UTC */
  reset_at: string | null;
  reset_description: string | null;
  exceeded: boolean;
  /** Cap on one model/feature; excluded from the headline percentage. */
  scoped: boolean;
}

export interface UsageSnapshot {
  provider_id: ProviderId;
  provider_name: string;
  status: UsageStatus;
  source: UsageSource;
  windows: UsageWindow[];
  plan_name: string | null;
  account_identifier: string | null;
  detail: string | null;
  message: string | null;
  fetched_at: string;
}

export interface ProviderView {
  provider_id: ProviderId;
  provider_name: string;
  enabled: boolean;
  snapshot: UsageSnapshot | null;
  status: UsageStatus;
  freshness: Freshness;
  last_error: string | null;
  last_attempt_at: string | null;
  next_due_at: string | null;
}
