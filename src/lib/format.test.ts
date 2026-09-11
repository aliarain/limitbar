import { describe, expect, it } from "vitest";
import { cardState, formatAge, formatPercent, formatReset, latestFetch, minRemaining, statusCopy } from "./format";
import type { ProviderView, UsageWindow } from "../types/usage";

const NOW = Date.parse("2026-09-11T12:00:00Z");

function win(remaining: number | null, resetAt: string | null = null): UsageWindow {
  return {
    id: "w",
    label: "W",
    used_percent: remaining === null ? null : 100 - remaining,
    remaining_percent: remaining,
    reset_at: resetAt,
    reset_description: null,
    exceeded: remaining === 0,
  };
}

function view(partial: Partial<ProviderView> & { windows?: UsageWindow[] }): ProviderView {
  const { windows, ...rest } = partial;
  const snapshot =
    windows === undefined
      ? null
      : {
          provider_id: "command-code" as const,
          provider_name: "Command Code",
          status: "AVAILABLE" as const,
          source: "OFFICIAL_API" as const,
          windows,
          plan_name: null,
          account_identifier: null,
          detail: null,
          message: null,
          fetched_at: new Date(NOW).toISOString(),
        };
  return {
    provider_id: "command-code",
    provider_name: "Command Code",
    enabled: true,
    snapshot,
    status: "AVAILABLE",
    freshness: "FRESH",
    last_error: null,
    last_attempt_at: null,
    next_due_at: null,
    ...rest,
  };
}

describe("formatPercent", () => {
  it("rounds and clamps", () => {
    expect(formatPercent(93.33)).toBe("93%");
    expect(formatPercent(0.4)).toBe("0%");
    expect(formatPercent(120)).toBe("100%");
    expect(formatPercent(-3)).toBe("0%");
  });
  it("never fabricates", () => {
    expect(formatPercent(null)).toBe("—");
    expect(formatPercent(Number.NaN)).toBe("—");
  });
});

describe("formatReset", () => {
  const at = (ms: number) => new Date(NOW + ms).toISOString();
  it("minutes", () => expect(formatReset(at(47 * 60_000), NOW)).toBe("Resets in 47m"));
  it("hours and minutes", () => expect(formatReset(at((2 * 60 + 14) * 60_000), NOW)).toBe("Resets in 2h 14m"));
  it("exact hours", () => expect(formatReset(at(3 * 3_600_000), NOW)).toBe("Resets in 3h"));
  // Anchor at local 09:00 so "+8h" and "+20h" land on today / tomorrow in any timezone.
  const localMorning = new Date(2026, 8, 11, 9, 0, 0).getTime();
  it("later today", () =>
    expect(formatReset(new Date(localMorning + 8 * 3_600_000).toISOString(), localMorning)).toMatch(/^Resets today at /));
  it("tomorrow", () =>
    expect(formatReset(new Date(localMorning + 20 * 3_600_000).toISOString(), localMorning)).toMatch(/^Resets tomorrow at /));
  it("a date further out", () => expect(formatReset(at(3.5 * 86_400_000), NOW)).toMatch(/^Resets Sep 1[45]$/));
  it("past resets are not negative", () => expect(formatReset(at(-5_000), NOW)).toBe("Resetting…"));
  it("null and garbage", () => {
    expect(formatReset(null, NOW)).toBeNull();
    expect(formatReset("not a date", NOW)).toBeNull();
  });
});

describe("formatAge", () => {
  it("seconds / minutes / hours", () => {
    expect(formatAge(new Date(NOW - 12_000).toISOString(), NOW)).toBe("Updated 12 sec ago");
    expect(formatAge(new Date(NOW - 18 * 60_000).toISOString(), NOW)).toBe("Updated 18 min ago");
    expect(formatAge(new Date(NOW - 3 * 3_600_000).toISOString(), NOW)).toBe("Updated 3 h ago");
    expect(formatAge(null, NOW)).toBeNull();
  });
});

describe("cardState", () => {
  it("normal / low / critical from the binding window", () => {
    expect(cardState(view({ windows: [win(100), win(72)] }))).toBe("normal");
    expect(cardState(view({ windows: [win(100), win(20)] }))).toBe("low");
    expect(cardState(view({ windows: [win(9), win(80)] }))).toBe("critical");
    expect(cardState(view({ windows: [win(0)] }))).toBe("critical");
  });
  it("loading before first result", () => {
    expect(cardState(view({ freshness: "NEVER", status: "UNAVAILABLE" }))).toBe("loading");
    expect(cardState(view({ freshness: "REFRESHING", status: "UNAVAILABLE" }))).toBe("loading");
  });
  it("stale keeps old data visible and wins over low/critical", () => {
    expect(cardState(view({ windows: [win(5)], freshness: "STALE", status: "ERROR" }))).toBe("stale");
  });
  it("auth required beats everything", () => {
    expect(cardState(view({ windows: [win(50)], status: "AUTH_REQUIRED", freshness: "STALE" }))).toBe("auth_required");
    expect(cardState(view({ status: "AUTH_REQUIRED", freshness: "NEVER" }))).toBe("auth_required");
  });
  it("unavailable when there is a snapshot but no numbers", () => {
    expect(cardState(view({ windows: [], status: "UNAVAILABLE" }))).toBe("unavailable");
    expect(cardState(view({ windows: [win(null)], status: "UNAVAILABLE" }))).toBe("unavailable");
  });
});

describe("minRemaining", () => {
  it("ignores nulls and picks the minimum", () => {
    expect(minRemaining(view({ windows: [win(null), win(40), win(90)] }))).toBe(40);
    expect(minRemaining(view({ windows: [win(null)] }))).toBeNull();
    expect(minRemaining(view({}))).toBeNull();
  });
});

describe("statusCopy", () => {
  it("covers statuses", () => {
    expect(statusCopy("AUTH_REQUIRED", "NEVER", null)).toBe("Sign in required");
    expect(statusCopy("ERROR", "STALE", "network error: timed out")).toBe("Refresh failed: network error: timed out");
    expect(statusCopy("ERROR", "STALE", null)).toBe("Refresh failed");
    expect(statusCopy("RATE_LIMITED", "STALE", null)).toMatch(/rate limiting/);
    expect(statusCopy("UNAVAILABLE", "NEVER", null)).toBeNull();
    expect(statusCopy("UNAVAILABLE", "FRESH", null)).toBe("Usage unavailable");
    expect(statusCopy("AVAILABLE", "FRESH", null)).toBeNull();
  });
});

describe("latestFetch", () => {
  it("returns the newest fetched_at", () => {
    const a = view({ windows: [win(1)] });
    const b = view({ windows: [win(1)] });
    b.snapshot!.fetched_at = new Date(NOW + 5000).toISOString();
    expect(latestFetch([a, b])).toBe(new Date(NOW + 5000).toISOString());
    expect(latestFetch([view({})])).toBeNull();
  });
});
