# LimitBar — Product Requirements Document (working name)

You are the senior engineer responsible for building a production-quality cross-platform desktop application called "LimitBar" (working name).

Do not blindly start implementing UI.

Your first responsibility is to investigate the feasibility of every major assumption, especially how provider usage and reset information can be obtained reliably.

## PRODUCT

LimitBar is a lightweight system-wide AI coding usage monitor.

It answers one question:

"How much usage do I have left across my AI coding tools, and when do those limits reset?"

The application runs continuously in the background.

Primary platforms:

1. macOS
2. Windows
3. Linux where practical

Primary providers for V1:

1. Claude Code
2. OpenAI Codex
3. Command Code
4. Gemini CLI if reliable integration is possible

Cursor and others can come later.

## CORE EXPERIENCE

macOS: LimitBar lives primarily in the macOS menu bar.

Windows: LimitBar lives in the Windows system tray.

Linux: Use the appropriate system tray/status notifier implementation where supported.

The user should NOT need to keep a normal application window open.

Example:

```
LIMITBAR

Claude Code
72% remaining
██████████████░░░░░
Resets in 2h 14m

Codex
43% remaining
████████░░░░░░░░░░░
Resets tomorrow at 5:00 AM

Command Code
91% remaining
██████████████████░░
Resets in 4d

Last updated 12 seconds ago
```

Clicking a provider can show more information.

The tray/menu-bar icon itself may optionally display aggregate/current-provider usage.

Example: `[◉ 67%]`

## OPTIONAL FLOATING WIDGET

The screenshot/concept that inspired the project uses a small indicator at the top center of the screen.

Implement this only after the tray/menu-bar experience works.

It should be optional.

Possible appearance:

```
┌──────────────────┐
│ Claude     62%   │
│ ███████████░░░   │
└──────────────────┘
```

Requirements:

- always-on-top
- frameless
- transparent where supported
- click-through option
- draggable/repositionable if appropriate
- hide automatically during fullscreen apps if feasible
- selectable provider
- disable completely from settings
- must not steal keyboard focus

Do NOT make this widget necessary for using LimitBar.

## TECH STACK

Use:

- Tauri 2
- Rust
- React
- TypeScript
- Vite
- minimal frontend dependencies

Do NOT use Electron.

The reason for Tauri is that this is a background utility expected to run all day. Startup time, memory usage and native OS behavior matter.

Keep the architecture simple.

Suggested structure:

```
src/
  components/
  views/
  hooks/
  stores/
  lib/
  types/

src-tauri/src/
  providers/
    mod.rs
    claude.rs
    codex.rs
    command_code.rs
    gemini.rs

  usage/
    mod.rs
    manager.rs
    models.rs

  platform/
    mod.rs
    tray.rs
    notifications.rs
    autostart.rs

  commands/
  config/
  lib.rs
  main.rs
```

## PROVIDER ARCHITECTURE

This is the most important architectural requirement.

Do NOT couple the application to individual provider implementations.

Create a provider abstraction.

Conceptually:

```rust
trait UsageProvider {
    async fn get_usage(&self) -> Result<UsageSnapshot>;
}
```

UsageSnapshot should represent something similar to:

```
{
    provider_id,
    provider_name,

    used_percent?,
    remaining_percent?,

    reset_at?,
    reset_description?,

    plan_name?,
    account_identifier?,

    status,

    source,

    fetched_at
}
```

Status:

```
AVAILABLE
UNAVAILABLE
AUTH_REQUIRED
RATE_LIMITED
ERROR
UNSUPPORTED
```

Source:

```
OFFICIAL_API
LOCAL_PROVIDER_DATA
LOCAL_CLI
ESTIMATED
```

However:

ESTIMATED MUST NEVER be presented to the user as authoritative.

If we cannot obtain a trustworthy percentage, show:

"Usage unavailable"

rather than inventing a value.

## PHASE 0 — INVESTIGATION

DO THIS BEFORE BUILDING THE FULL APPLICATION.

Investigate each provider separately.

For:

- Claude Code
- OpenAI Codex
- Command Code
- Gemini CLI

Determine:

1. Is there an official API for usage?
2. Is remaining subscription usage available?
3. Is reset time available?
4. Does the local CLI expose this information?
5. Is there legitimate local state/cache containing this information?
6. Can usage be retrieved without asking users to paste API keys?
7. What authentication method is required?
8. Does the method work with subscription users rather than only API billing?
9. Is the information authoritative or inferred?
10. How likely is the integration to break?

DO NOT:

- scrape random website HTML
- automate browser dashboards
- reverse engineer encrypted credentials
- bypass provider security controls
- transmit credentials to our servers
- pretend token counts equal subscription quota
- claim estimated data is exact

Create: `docs/provider-feasibility.md`

For every provider document:

- AVAILABLE DATA
- AUTH METHOD
- SOURCE
- RELIABILITY
- LIMITATIONS
- IMPLEMENTATION PLAN

Then classify integrations:

- A — reliable
- B — usable with limitations
- C — experimental
- D — cannot reliably support

Only implement A/B integrations in V1.

## COMMAND CODE

Command Code should be treated as a first-class provider.

If this repository has access to Command Code APIs/contracts, investigate the proper authenticated mechanism.

If no usage API currently exists, clearly document what endpoint/data contract Command Code would need.

Do not invent an endpoint.

Ideal contract would provide:

- remaining percentage
- used percentage
- limit window
- reset timestamp
- plan
- account

But adapt to whatever actually exists.

## LOCAL-FIRST

LimitBar should be local-first.

Provider credentials and session information should stay on the machine.

Never send provider credentials through a LimitBar-owned server merely to retrieve usage.

Prefer: `provider -> local LimitBar process -> UI`

not: `provider -> LimitBar server -> desktop`

If sensitive tokens must be persisted, use OS credential storage/keychain rather than plaintext configuration.

Never log credentials.

## POLLING

Build a UsageManager.

It owns provider state.

Responsibilities:

- fetch usage
- cache latest successful snapshot
- prevent duplicate concurrent requests
- handle provider failures independently
- exponential backoff
- update UI
- trigger threshold notifications

Do NOT poll providers aggressively.

Default interval: 5 minutes

Allow manual refresh.

Potential states:

- fresh
- refreshing
- stale
- offline
- auth required
- provider unavailable

If a refresh fails but previous data exists: show previous data as stale.

Example:

```
Claude
72%
Updated 18 minutes ago
Refresh failed
```

Do not replace useful cached data with zero.

## RESET HANDLING

Reset time is a core feature.

Store reset timestamps internally as absolute timestamps.

Render them locally.

Examples:

```
Resets in 47m
Resets in 2h 14m
Resets today at 8:00 PM
Resets tomorrow at 5:00 AM
Resets Sep 14
```

When the reset passes, refresh the provider.

## NOTIFICATIONS

Support configurable notifications.

Defaults:

- 20% remaining
- 10% remaining
- 5% remaining
- usage reset

Example:

```
Claude Code is below 10%.
Codex still has 76% remaining.
```

Or:

```
Claude Code usage has reset.
```

Prevent duplicate notifications within the same usage window.

## SMART PROVIDER SUGGESTION

Do NOT build an AI recommendation system.

A deterministic rule is enough.

If:

```
Claude = 8%
Codex = 74%
Gemini = 91%
```

the UI can show:

"Codex and Gemini have significantly more capacity."

Do not claim one model is "better" merely because it has more remaining usage.

## V1 UI

Keep the UI extremely small.

Tray popup:

Header

- LimitBar
- Refresh button
- Settings button

Provider cards

```
Claude Code          72%
████████████████░░░░
Resets in 2h 14m

Codex                43%
█████████░░░░░░░░░░░
Resets tomorrow 5:00 AM

Command Code         91%
██████████████████░░
Resets in 4d
```

Footer

```
Updated 12 sec ago
```

Provider card states must include:

- normal
- low
- critical
- loading
- stale
- unavailable
- authentication required

Do not fill the application with gradients, glassmorphism, giant cards or dashboard-style UI.

This is a utility.

Think: Raycast, Linear, native macOS menu utilities.

Compact. Fast. Readable.

## SETTINGS

V1 settings:

General
- Launch at login
- Start minimized
- Update interval

Providers
- Enable/disable provider
- Authentication/status
- Refresh/test connection

Notifications
- 20%
- 10%
- 5%
- reset notification

Floating Indicator
- enabled
- provider
- position
- click-through
- opacity if appropriate

Appearance
- system
- light
- dark

## TRAY BEHAVIOR

Left click: open usage popover/window

Right click: native menu

Possible menu:

```
LimitBar
────────────
Claude       72%
Codex        43%
Command Code 91%
────────────
Refresh
Open LimitBar
Settings
Quit
```

Do not perform network operations directly inside tray rendering.

Tray reads state from UsageManager.

## PERFORMANCE

This app should feel almost invisible when idle.

Targets, not absolute guarantees:

- Idle CPU: approximately 0%
- Memory: keep it materially below a typical Electron application.

No permanent busy loops.

No 1-second provider polling.

Countdown displays can update locally without fetching provider data.

## SECURITY

Treat local provider authentication as sensitive.

Requirements:

- never print tokens
- redact secrets from errors
- no telemetry containing credentials
- no plaintext credential persistence
- strict Tauri capabilities
- minimum filesystem permissions
- minimum network permissions
- validate IPC inputs
- frontend cannot arbitrarily read filesystem
- frontend cannot arbitrarily execute commands

Do not use shell commands when a native/library implementation is available.

If invoking an installed CLI is genuinely necessary, use a tightly scoped command with validated arguments.

## ERROR HANDLING

One provider failing must NEVER break the application.

Examples:

- Claude works
- Codex auth expired
- Command Code API unavailable

The app should simply display those three states independently.

No crash. No endless loading spinner. No destructive credential clearing automatically.

## DATA MODEL

Create stable provider IDs:

```
claude
codex
command-code
gemini
```

Do not use display names as identifiers.

Persist:

- enabled providers
- preferences
- notification thresholds
- floating widget settings
- latest non-sensitive cache

Do not persist raw provider credentials in normal application configuration.

## TESTING

Rust — unit tests for:
- usage normalization
- reset calculations
- stale-state logic
- threshold notifications
- provider failure handling

Frontend — tests for:
- provider states
- percentage rendering
- unavailable usage
- reset countdown
- stale state

Integration: test provider adapters separately.

Do not require real provider credentials for the normal test suite. Create mocks/fixtures.

## PLATFORM BEHAVIOR

macOS:

Primary experience: menu bar

Support:
- launch at login
- native notifications
- hide Dock icon where appropriate
- popover/window near menu bar
- optional floating overlay

Windows:

Primary experience: system tray

Support:
- launch at login
- native notifications
- small popup window
- optional top-center floating overlay

Linux:

Support tray functionality where practical.

Do not compromise macOS/Windows quality merely to support every Linux desktop environment.

## V1 NON-GOALS

DO NOT BUILD:

- accounts
- LimitBar cloud backend
- team dashboards
- billing
- social features
- historical analytics dashboards
- model benchmarking
- prompt history
- token optimization
- AI chat
- coding agent
- provider switching automation
- automatic modification of other applications
- mobile apps
- browser extension

Keep V1 brutally small.

## IMPLEMENTATION ORDER

1. Inspect repository/environment.
2. Research provider feasibility.
3. Write: docs/provider-feasibility.md
4. Define provider abstraction and UsageSnapshot.
5. Implement ONE provider end-to-end.
6. Build UsageManager.
7. Build macOS menu bar/tray experience.
8. Add second provider.
9. Add Command Code.
10. Add Windows tray support.
11. Implement notifications.
12. Implement autostart.
13. Add settings.
14. Add optional floating indicator.
15. Test packaging/install/update behavior.
16. Only then consider additional providers.

## IMPORTANT DEVELOPMENT RULES

- Do not rewrite working infrastructure without evidence.
- Do not add abstractions before they are required.
- Do not install large dependencies for trivial functionality.
- Do not silently swallow errors.
- Do not leave TODO placeholders pretending features work.
- Do not mock provider usage in production.
- Do not display fabricated percentages.
- Do not claim an integration works until tested against the actual provider.
- Before modifying architecture, trace the existing code and state ownership.
- Prefer the smallest correct implementation.
- Delete unnecessary complexity rather than wrapping it in another abstraction.

## DELIVERABLE

I want a working application, not a prototype screenshot.

At the end I should be able to:

1. Install LimitBar.
2. Launch it.
3. See it in my macOS menu bar or Windows tray.
4. Connect supported providers.
5. See trustworthy remaining usage where available.
6. See the next reset time where available.
7. Manually refresh.
8. Receive low-usage/reset notifications.
9. Restart my computer and have LimitBar launch automatically if enabled.
10. Leave it running all day without noticeable resource usage.

## FIRST ACTION

Do not start by designing the interface.

Start by investigating Claude Code, Codex and Command Code usage availability.

Produce docs/provider-feasibility.md.

Then tell me:

- which providers can reliably work
- exactly where the data comes from
- what authentication is required
- what information each provider exposes
- what cannot be obtained reliably
- which provider you recommend implementing first

After that, proceed with the smallest production-quality implementation unless a fundamental feasibility blocker makes the product impossible.

## SESSION NOTE (2026-09-11)

Owner instruction: investigate and implement **Command Code first**, before any other provider.
