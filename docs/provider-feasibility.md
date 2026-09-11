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
| Claude Code  | **A** | Investigated + verified live 2026-09-11  |
| OpenAI Codex | **A** | Investigated + verified live 2026-09-11  |
| Gemini CLI   | —     | Deferred (not in current scope)          |

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
- The CLI sends `x-command-code-version` / `x-cli-environment` headers. **Verified not
  required** — LimitBar sends only `Authorization` and an honest `User-Agent:
  LimitBar/<version>`; it does not impersonate the CLI.
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

### HOW A NORMAL USER GETS ACCESS

Verified on this machine: both the CLI (`command-code` npm package) and the desktop app
(`Command Code.app`, bundle `ai.commandcode.desktop`) read the **same** file,
`~/.commandcode/auth.json`, via a shared harness. The desktop app also consumes
`windowLimits` from `/alpha/billing/credits` for its own popover usage meter.

So for LimitBar users the onboarding is:

| User situation                                        | What LimitBar does                                  | User effort |
|-------------------------------------------------------|-----------------------------------------------------|-------------|
| Has used Command Code CLI **or** desktop app and logged in | Auto-detects `~/.commandcode/auth.json`, shows usage | **None**    |
| Has Command Code installed but never logged in        | Card shows "Sign in required" with a button that opens the CLI login (`cmd login`) or the desktop app | One click + browser login |
| Doesn't have Command Code at all                      | Card shows "Command Code not installed" with link to commandcode.ai; provider can be disabled | n/a |
| Wants LimitBar on a machine without Command Code (e.g. a second Mac) | Settings → Providers → Command Code → "Paste API key" (created in Studio → Settings → API keys); stored in OS keychain | Paste once |

Notes:

- The CLI honours no env var for the key; `auth.json` is the single source.
  Key file permissions on this machine: user-readable JSON (the vendor's choice).
- LimitBar reads the file into memory on each refresh (so `cmd login` / `cmd logout`
  are picked up without restart), never copies it elsewhere, never logs it.
- If the file disappears (user ran `cmd logout`), LimitBar transitions to
  `AUTH_REQUIRED` and keeps the last snapshot marked stale — no data is deleted.

### LEGAL / TERMS OF SERVICE (reviewed 2026-09-11, ToS last updated 2026-07-03)

Not legal advice. Facts relevant to `commandcode.ai/terms`:

- **Favourable:** the user's own key reads the user's own billing status on the user's
  own machine; identical endpoint/auth/traffic to the vendor's CLI and desktop app; one
  request set per 5 minutes; nothing bypassed (revoked key → 401).
- **Gray:** ToS bars automated requests / data extraction from "the Company's website"
  (LimitBar calls the API, not the website); ToS bars reverse engineering (the endpoint
  was located by reading the vendor's shipped, unobfuscated JS — interoperability
  inspection, but the wording is broad); `/alpha/*` is not a published third-party API.
- **Verified:** endpoints respond normally with an honest `User-Agent: LimitBar/<ver>`
  and **no** CLI version headers. LimitBar identifies itself truthfully and never
  impersonates the CLI or desktop app.

Policy for this project:

1. Personal / development use proceeds now.
2. **Obtain written permission from Command Code before public distribution.**
   Ask for (a) confirmation that third-party read-only use of
   `/alpha/billing/credits` with a user's own key is acceptable, and (b) ideally a
   stable, documented endpoint. Track in `docs/provider-feasibility.md` when answered.
3. Good-citizen behaviour is non-negotiable: honest UA, ≥5-minute interval, honour
   `429` / `Retry-After` with backoff, stop polling on repeated `401`, only the three
   read-only GETs, no scraping of `commandcode.ai` pages.

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

## Claude Code — Class A

Investigated 2026-09-11 against Claude Code 2.1.259 (native binary) and the live
`https://api.anthropic.com` OAuth API, using the investigator's own Claude Max login.

### AVAILABLE DATA

`GET https://api.anthropic.com/api/oauth/usage` — the endpoint the official CLI's
`/usage` command uses (string confirmed present in the shipped binary alongside the
`anthropic-ratelimit-unified-5h/7d-*` response headers it also consumes). Verified live
response (abridged, real numbers):

```json
{
  "five_hour": { "utilization": 3,  "resets_at": "2026-09-11T14:00:00.319696+00:00", "locked_reason": null },
  "seven_day": { "utilization": 61, "resets_at": "2026-09-13T03:00:00.319714+00:00", "locked_reason": null },
  "seven_day_opus": null, "seven_day_sonnet": null,
  "limits": [
    { "kind": "session",       "group": "session", "percent": 3,   "severity": "normal",   "resets_at": "…", "scope": null, "is_active": false },
    { "kind": "weekly_all",    "group": "weekly",  "percent": 61,  "severity": "normal",   "resets_at": "…", "scope": null, "is_active": false },
    { "kind": "weekly_scoped", "group": "weekly",  "percent": 100, "severity": "critical", "resets_at": "…",
      "scope": { "model": { "display_name": "Fable" } }, "is_active": true }
  ],
  "extra_usage": { "is_enabled": false, "utilization": null }
}
```

| LimitBar field        | Source                                         | Authoritative? |
|-----------------------|------------------------------------------------|----------------|
| used_percent (5h)     | `five_hour.utilization` (integer %)            | Yes — server   |
| used_percent (weekly) | `seven_day.utilization`                        | Yes — server   |
| reset_at              | `*.resets_at` (RFC 3339 with offset)           | Yes — server   |
| per-model weekly caps | `limits[]` entries with `scope.model`          | Yes — server   |
| exceeded / locked     | `locked_reason`, `limits[].severity`           | Yes — server   |
| plan_name             | `GET /api/oauth/profile` → `organization.rate_limit_tier` (e.g. `default_claude_max_20x`); keychain `subscriptionType` (`max`/`pro`) | Yes |
| account_identifier    | `profile.account.email` (masked in UI)         | Yes            |

`utilization` is an integer percent already; no client-side arithmetic. Fields with
opaque codenames (`tangelo`, `nimbus_quill`, …) are ignored — unknown fields must never
be rendered.

### AUTH METHOD

- OAuth bearer token issued by `claude login` (Claude.ai account; works for Pro/Max
  subscriptions — verified with a Max account). Headers: `Authorization: Bearer <token>`,
  `anthropic-beta: oauth-2025-04-20`.
- Storage: **macOS Keychain** generic password, service `Claude Code-credentials`,
  account = the macOS username; value is JSON
  `{ "claudeAiOauth": { accessToken, refreshToken, expiresAt, scopes, subscriptionType, rateLimitTier } }`.
  Linux/Windows: `~/.claude/.credentials.json` with the same JSON.
- The access token is short-lived (hours). **LimitBar never refreshes it**: rotating the
  refresh token from a second client risks invalidating Claude Code's own session. When
  `expiresAt` has passed → `AUTH_REQUIRED` with "Open Claude Code to refresh sign-in";
  the CLI refreshes on its next run and LimitBar picks the new token up on its next poll.
- Keychain read triggers a one-time macOS consent dialog ("LimitBar wants to access…");
  "Always Allow" persists for a signed build. Dev builds re-prompt after each rebuild.
- 401 → `AUTH_REQUIRED`. No token is ever written by LimitBar.

### SOURCE

`OFFICIAL_API` — the vendor's own OAuth usage endpoint, same as the CLI's `/usage`.

### RELIABILITY

- Undocumented for third parties, but the CLI depends on it and several community
  menu-bar tools have used it for months. Breakage risk: moderate; the response has
  many nullable/experimental fields, so parsing is tolerant and only `five_hour`,
  `seven_day`, `limits[]` are used.
- Latency ~0.6–0.8 s. Poll every 5 min.

### LIMITATIONS

- No documented public API contract; Anthropic's consumer terms also bar reverse
  engineering — LimitBar reads a shipped string table only, and the endpoint is the one
  the community has already documented. Same policy as Command Code: personal use now,
  seek written OK before wide distribution.
- Token expiry between Claude Code runs shows as "sign-in needed" even though the user
  is still logged in; the copy must say to open Claude Code, not to log in again.
- Keychain access from an unsigned dev binary prompts repeatedly.

### IMPLEMENTATION PLAN

1. Provider id `claude`, name "Claude Code". Credential: Keychain (macOS) / file (other).
2. One `GET /api/oauth/usage` per poll; `GET /api/oauth/profile` cached ~1 h for plan.
3. Windows: `five_hour` (primary), `seven_day`, then any `limits[]` with
   `group == "weekly"` and a model scope, labelled by `scope.model.display_name`.
4. `locked_reason != null` or `severity == "critical"` → `exceeded = true`.

---

## OpenAI Codex — Class A

Investigated 2026-09-11 against codex-cli 0.144.1 (native binary) and the live
`https://chatgpt.com/backend-api` service, using the investigator's own ChatGPT Pro login.

### AVAILABLE DATA

`GET https://chatgpt.com/backend-api/wham/usage` (alias `codex/usage`; both strings are
present in the shipped binary and returned identical data). Verified live response
(abridged):

```json
{
  "plan_type": "pro",
  "rate_limit": {
    "allowed": true, "limit_reached": false,
    "primary_window":   { "used_percent": 66, "limit_window_seconds": 604800, "reset_after_seconds": 315566, "reset_at": 1789435681 },
    "secondary_window": null
  },
  "additional_rate_limits": [
    { "limit_name": "GPT-5.3-Codex-Spark", "rate_limit": { "primary_window": { "used_percent": 0, "limit_window_seconds": 18000, "reset_at": 1789138115 },
                                                            "secondary_window": { "used_percent": 0, "limit_window_seconds": 604800, "reset_at": 1789724915 } } }
  ],
  "credits": { "has_credits": false, "unlimited": false, "balance": "0" },
  "rate_limit_reached_type": null
}
```

| LimitBar field   | Source                                                             | Authoritative? |
|------------------|--------------------------------------------------------------------|----------------|
| used_percent     | `rate_limit.{primary,secondary}_window.used_percent`               | Yes — server   |
| window label     | `limit_window_seconds` (18000 → "5h", 604800 → "Week")              | Yes            |
| reset_at         | `reset_at` (epoch **seconds**)                                     | Yes — server   |
| exceeded         | `limit_reached`, `rate_limit_reached_type`                          | Yes            |
| plan_name        | `plan_type` (`pro`, `plus`, `team`, …)                              | Yes            |
| account_identifier | `email` (masked in UI)                                            | Yes            |
| model-specific   | `additional_rate_limits[]` (secondary, shown on expand)             | Yes            |

Which of primary/secondary is the 5-hour window varies by plan (this Pro account
currently reports only a weekly primary window). LimitBar classifies by
`limit_window_seconds`, never by position.

### AUTH METHOD

- OAuth bearer token from `codex login` (ChatGPT account). Headers:
  `Authorization: Bearer <access_token>`, `ChatGPT-Account-Id: <account_id>`.
- Storage: `~/.codex/auth.json` →
  `{ auth_mode: "chatgpt", tokens: { id_token, access_token, refresh_token, account_id }, last_refresh }`.
  `access_token` is a JWT; `exp` claim verified ~10 days out. Codex refreshes it itself.
  LimitBar never refreshes; expired → `AUTH_REQUIRED` ("Run codex to refresh sign-in").
- `auth_mode: "apikey"` (API-key users) has no subscription quota → `UNSUPPORTED`
  with "Codex API-key mode has no usage limits to show".
- Works for subscription users (verified with Pro). 401 → `AUTH_REQUIRED`.

### SOURCE

`OFFICIAL_API` — the endpoint the CLI's own `/status` limits display uses.

### RELIABILITY

- Undocumented for third parties; used by the vendor CLI and by community tools.
  Breakage risk: moderate. Latency 0.6–1.7 s.

### LIMITATIONS

- Same third-party-use caveat as the others (OpenAI terms bar reverse engineering; only a
  string table was inspected, and the endpoint is community-documented).
- The 5h/weekly split is plan-dependent; UI must cope with one or two windows.

### IMPLEMENTATION PLAN

1. Provider id `codex`, name "Codex". Credential: `~/.codex/auth.json` only.
2. One `GET wham/usage` per poll. Windows: primary + secondary (when present) sorted
   5h first; `additional_rate_limits` folded in as extra windows labelled by
   `limit_name` (weekly windows only, to keep the card small).
3. `limit_reached` → mark the matching window `exceeded`.

---

## Gemini CLI

_Deferred — not in current scope (owner decision 2026-09-11)._
