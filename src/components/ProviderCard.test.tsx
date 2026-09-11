import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ProviderCard } from "./ProviderCard";
import type { ProviderView, UsageWindow } from "../types/usage";

const NOW = Date.parse("2026-09-11T12:00:00Z");

function win(id: string, label: string, remaining: number | null, resetInMs: number | null): UsageWindow {
  return {
    id,
    label,
    used_percent: remaining === null ? null : 100 - remaining,
    remaining_percent: remaining,
    reset_at: resetInMs === null ? null : new Date(NOW + resetInMs).toISOString(),
    reset_description: null,
    exceeded: remaining === 0,
  };
}

function view(over: Partial<ProviderView>, windows?: UsageWindow[]): ProviderView {
  return {
    provider_id: "command-code",
    provider_name: "Command Code",
    enabled: true,
    snapshot:
      windows === undefined
        ? null
        : {
            provider_id: "command-code",
            provider_name: "Command Code",
            status: "AVAILABLE",
            source: "OFFICIAL_API",
            windows,
            plan_name: "GOAT",
            account_identifier: "someone",
            detail: "67.7 credits left · period ends Oct 5",
            message: null,
            fetched_at: new Date(NOW).toISOString(),
          },
    status: "AVAILABLE",
    freshness: "FRESH",
    last_error: null,
    last_attempt_at: null,
    next_due_at: null,
    ...over,
  };
}

describe("ProviderCard", () => {
  afterEach(cleanup);

  it("renders percentage, windows and reset countdown", () => {
    render(<ProviderCard now={NOW} view={view({}, [win("five_hour", "5h", 100, null), win("weekly", "Week", 93.3, 2 * 3_600_000 + 14 * 60_000)])} />);
    expect(screen.getByText("Command Code")).toBeTruthy();
    expect(screen.getByText("93%")).toBeTruthy(); // binding window in header
    expect(screen.getByText(/resets in 2h 14m/)).toBeTruthy();
    expect(screen.getByText(/idle/)).toBeTruthy();
    expect(screen.getByText("GOAT · @someone")).toBeTruthy();
    expect(screen.getByText(/67.7 credits left/)).toBeTruthy();
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("93"); // bar tracks the binding window
    expect(screen.getByLabelText("Command Code").getAttribute("data-state")).toBe("normal");
  });

  it("shows low and critical states", () => {
    const { rerender } = render(<ProviderCard now={NOW} view={view({}, [win("a", "A", 15, 60_000)])} />);
    expect(screen.getByLabelText("Command Code").getAttribute("data-state")).toBe("low");
    rerender(<ProviderCard now={NOW} view={view({}, [win("a", "A", 4, 60_000)])} />);
    expect(screen.getByLabelText("Command Code").getAttribute("data-state")).toBe("critical");
  });

  it("keeps stale data visible with a failure note", () => {
    render(
      <ProviderCard now={NOW} view={view({ freshness: "STALE", status: "ERROR", last_error: "network error: timed out" }, [win("a", "A", 72, 60_000)])} />,
    );
    expect(screen.getByText("72%")).toBeTruthy();
    expect(screen.getByText(/Refresh failed: network error: timed out/)).toBeTruthy();
    expect(screen.getByLabelText("Command Code").getAttribute("data-state")).toBe("stale");
  });

  it("shows 'Usage unavailable' instead of a number when there is none", () => {
    render(<ProviderCard now={NOW} view={view({ status: "UNAVAILABLE" }, [])} />);
    expect(screen.getByText("—")).toBeTruthy();
    expect(screen.getAllByText(/Usage unavailable/).length).toBeGreaterThan(0);
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("shows sign-in state without any percentage", () => {
    render(<ProviderCard now={NOW} view={view({ status: "AUTH_REQUIRED", freshness: "NEVER" })} />);
    expect(screen.getByText("Sign in required")).toBeTruthy();
    expect(screen.getByLabelText("Command Code").getAttribute("data-state")).toBe("auth_required");
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("shows loading before the first fetch", () => {
    render(<ProviderCard now={NOW} view={view({ status: "UNAVAILABLE", freshness: "NEVER" })} />);
    expect(screen.getByText("…")).toBeTruthy();
    expect(screen.getByText("Loading…")).toBeTruthy();
  });
});
