import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Chip } from "./Chip";
import type { ProviderView } from "../types/usage";

function view(id: ProviderView["provider_id"], remaining: number | null, over: Partial<ProviderView> = {}): ProviderView {
  return {
    provider_id: id,
    provider_name: id,
    enabled: true,
    snapshot:
      remaining === null
        ? null
        : {
            provider_id: id, provider_name: id, status: "AVAILABLE", source: "OFFICIAL_API",
            windows: [{ id: "w", label: "W", used_percent: 100 - remaining, remaining_percent: remaining, reset_at: null, reset_description: null, exceeded: false, scoped: false }],
            plan_name: null, account_identifier: null, detail: null, message: null, fetched_at: new Date().toISOString(),
          },
    status: "AVAILABLE",
    freshness: "FRESH",
    last_error: null,
    last_attempt_at: null,
    next_due_at: null,
    ...over,
  };
}

describe("Chip", () => {
  afterEach(cleanup);

  it("shows short name, bar and percent per provider", () => {
    render(<Chip views={[view("command-code", 93.3), view("claude", 39), view("codex", 34)]} />);
    expect(screen.getByText("Cmd")).toBeTruthy();
    expect(screen.getByText("Claude")).toBeTruthy();
    expect(screen.getByText("Codex")).toBeTruthy();
    expect(screen.getByText("93%")).toBeTruthy();
    expect(screen.getByText("39%")).toBeTruthy();
    expect(screen.getByText("34%")).toBeTruthy();
    expect(document.querySelectorAll(".chip__fill").length).toBe(3);
  });

  it("marks states and never draws a bar without a number", () => {
    render(<Chip views={[view("claude", 5), view("codex", null, { status: "AUTH_REQUIRED", freshness: "NEVER" }), view("command-code", null, { status: "UNAVAILABLE", freshness: "NEVER" })]} />);
    expect(document.querySelector('[data-state="critical"]')).toBeTruthy();
    expect(screen.getByText("⚠")).toBeTruthy();
    expect(screen.getByText("…")).toBeTruthy();
    expect(document.querySelectorAll(".chip__fill").length).toBe(1);
  });
});
