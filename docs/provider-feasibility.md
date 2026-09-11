# Provider Feasibility

Phase 0 investigation for LimitBar. Each provider is investigated separately and
classified before any implementation work.

Classification:

- **A** — reliable
- **B** — usable with limitations
- **C** — experimental
- **D** — cannot reliably support

Only A/B integrations ship in V1.

| Provider     | Class | Status                                   |
|--------------|-------|------------------------------------------|
| Command Code | **A** | Investigated + verified live 2026-09-11  |
| Claude Code  | —     | Not yet investigated                     |
| OpenAI Codex | —     | Not yet investigated                     |
| Gemini CLI   | —     | Not yet investigated                     |

---

## Command Code — Class A

Investigated 2026-09-11 against `command-code@0.26.0` (npm, global) and the live
`https://api.commandcode.ai` service, using the investigator's own existing CLI login.

### AVAILABLE DATA

Command Code's public docs (`commandcode.ai/docs/resources/usage-limits`) describe the
limit model:

> "Every plan adds two rolling windows on top of its monthly credits: a 5-hour cap and
> a weekly cap." … "opens on your first request and resets exactly 5 hours (or 7 days)
> later." … "Usage never carries between windows. Each new one starts at zero."

The official CLI retrieves this from `GET https://api.commandcode.ai/alpha/billing/credits`.
Verified live response shape (numbers are real, identifiers redacted):

```json
{
  "credits": {
    "belowThreshold": false,
    "creditThreshold": 0,
    "monthlyCredits": 67.67,
    "purchasedCredits": 0,
    "freeCredits": 0.077
  },
  "windowLimits": {
    "limited": true,
    "exceeded": null,
    "fiveHour": { "used": 0,    "cap": 14, "exceeded": false, "resetAt": 0 },
    "weekly":   { "used": 2.33, "cap": 35, "exceeded": false, "resetAt": 1789424492261 }
  }
}
```

What each field gives LimitBar:

| LimitBar field         | Source                                              | Authoritative? |
|------------------------|-----------------------------------------------------|----------------|
| used_percent (5h)      | `windowLimits.fiveHour.used / cap * 100`            | Yes — server   |
| used_percent (weekly)  | `windowLimits.weekly.used / cap * 100`              | Yes — server   |
| reset_at (5h)          | `windowLimits.fiveHour.resetAt` (epoch **ms**)      | Yes — server   |
| reset_at (weekly)      | `windowLimits.weekly.resetAt` (epoch **ms**)        | Yes — server   |
| limit exceeded         | `windowLimits.*.exceeded`, `windowLimits.exceeded`  | Yes — server   |
| monthly credits left   | `credits.monthlyCredits` (+ `purchasedCredits`)     | Yes — server   |
| plan_name              | `GET /alpha/billing/subscriptions` → `data.planId`  | Yes — server   |
| billing period end     | `subscriptions` → `data.currentPeriodEnd` (ISO)     | Yes — server   |
| account_identifier     | `GET /alpha/whoami` → `user.userName`, `org.login`  | Yes — server   |

Semantics verified:

- `resetAt` is **epoch milliseconds** (`1789424492261` → `2026-09-14T22:21:32Z`, ~3.5 days
  after the probe; consistent with a 7-day window opened on first request).
- `resetAt === 0` together with `used === 0` means **no window is open** (nothing has
  been spent since the last reset). LimitBar must render this as "100% remaining, no
  active window", not as "resets at 1970-01-01".
- `cap` is in credits (on GOAT/Pro "usage-value units", on other plans 1 credit = $1).
  Percentages are plan-independent because `cap` comes from the server.
- `windowLimits.limited: false` would mean the account has no window caps
  (docs: "Extra pay-as-you-go credits are never capped"). Handle by reporting
  `status: UNAVAILABLE` for window meters while still showing monthly credits.

Additional endpoints available but **not needed for V1**:

- `GET /alpha/usage/summary?since=<ISO>` — billing-period totals (tokens, cost, request
  count). Informational only; not a quota.
- The CLI ships a hard-coded plan table (`individual-go/pro/max/ultra`, `teams-pro`) to
  compute a *monthly* depletion percentage. **Do not copy it.** The investigator's live
  plan is `individual-goat`, which is absent from that table — the CLI's own monthly
  percentage silently returns `null` for it. A client-side plan table is a stale-data
  trap. LimitBar uses `windowLimits` (server-provided `cap`) and never a local table.

### AUTH METHOD

- Bearer API key: `Authorization: Bearer <key>`.
- Key is created by `cmd login` (browser OAuth flow against `commandcode.ai/studio`) and
  written to `~/.commandcode/auth.json` as
  `{ apiKey, userId, userName, keyName, authenticatedAt }`. Keys are prefixed `user_`.
  (Staging/local variants: `auth.staging.json`, `auth.local.json` — ignore.)
- The CLI also sends `x-command-code-version: <cli version>` and
  `x-cli-environment: production`. Not verified as required, but sent by LimitBar for
  parity so server-side version gating behaves the same as for the CLI.
- Env override honoured by the CLI: `COMMANDCODE_API_URL` (only when
  `COMMANDCODE_SANDBOX=true`). LimitBar does not need it.
- Works for **subscription users** (verified with an active `individual-goat`
  subscription). No API-billing account needed. No key pasting needed if the user has
  run `cmd login`; LimitBar can also accept a pasted key as a fallback and store it in
  the OS keychain.
- Unauthenticated / invalid key → `401 {"success":false,"error":{"code":"UNAUTHORIZED",…}}`.
  Map to `AUTH_REQUIRED`. Never delete `auth.json` — it is the CLI's file, not ours.

### SOURCE

`OFFICIAL_API` — same authenticated endpoint the vendor's own CLI calls. Not scraped,
not inferred, not estimated.

### RELIABILITY

- Path prefix is `/alpha/`, i.e. vendor-labelled pre-stable. Breakage risk: moderate on
  a 6–12 month horizon; low day-to-day (the vendor's shipped CLI depends on the same
  contract and would break first).
- Response latency observed: 190 ms – 1.6 s. Polling every 5 min is negligible load.
- No rate-limit headers observed on these endpoints.
- `windowLimits` is **not** in the CLI's zod schema for this endpoint (the CLI only
  types `credits`). It is present in live responses and documented behaviourally in the
  public docs. LimitBar parses it defensively: missing `windowLimits` → window meters
  `UNAVAILABLE`, monthly credits still shown.
- Mitigation for drift: tolerant deserialisation (unknown fields ignored, missing
  fields → `UNAVAILABLE`, never `0`), and a contract fixture test pinned to the verified
  response above so schema changes surface as a failing test, not a wrong percentage.

### LIMITATIONS

- No documented public API contract; relies on the CLI's contract. If Command Code
  publishes a stable `/v1` usage endpoint, migrate.
- Monthly credit *percentage* is not computable without a plan total; the API returns
  remaining `monthlyCredits` but not the plan's monthly allowance. Show monthly credits
  as an absolute number ("67.7 credits left, period ends Oct 5"), not a bar.
- Reading `~/.commandcode/auth.json` means LimitBar handles a live credential. It must
  be read into memory only, never persisted by LimitBar, never logged, and redacted from
  every error path.
- Org accounts: pass `?orgId=<org.id>` from `whoami` when `org` is non-null (matches
  CLI behaviour). Not verified live (investigator has no org) — treat org support as
  **B** until tested.

### IMPLEMENTATION PLAN

1. Provider id `command-code`, display name "Command Code".
2. Credential resolution order: (a) LimitBar keychain entry (user-pasted key),
   (b) `~/.commandcode/auth.json` → `apiKey`. Neither is written by LimitBar except (a).
3. Fetch sequence per refresh (3 requests, all `GET`, `reqwest` with 15 s timeout):
   `whoami` → `billing/subscriptions[?orgId]` → `billing/credits[?orgId]`.
   `whoami`/`subscriptions` may be cached for an hour; `credits` every interval.
4. Normalise to `UsageSnapshot`:
   - Primary meter = **5-hour window** (the one users hit first). Secondary = weekly.
     Both exposed; UI shows primary with secondary on expand.
   - `used_percent = used / cap * 100`, clamped 0–100.
   - `reset_at = resetAt ms → UTC timestamp`, `None` when `resetAt == 0`.
   - `plan_name` from `planId` via a **display-name-only** map with passthrough for
     unknown ids (`individual-goat` → "GOAT"; unknown → raw id). Never used for math.
   - `status`: 401 → `AUTH_REQUIRED`; network/5xx → `ERROR` (keep stale cache);
     `windowLimits` missing or `limited:false` → `UNAVAILABLE` for meters.
5. Fixtures: the verified response above (+ a `resetAt: 0` case, a `limited:false`
   case, a 401 body) as JSON in `src-tauri/fixtures/command-code/`. Unit tests run
   against fixtures; a separate `--ignored` integration test hits the live API using
   the machine's own `auth.json` when present.

**Recommendation:** implement Command Code first. It is the only provider so far with a
verified, authoritative, server-computed percentage *and* absolute reset timestamp for
both windows, obtainable from an existing local login with zero user setup.

---

## Claude Code

_Not yet investigated (owner directed Command Code first)._

## OpenAI Codex

_Not yet investigated._

## Gemini CLI

_Not yet investigated._
