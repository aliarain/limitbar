# LimitBar

A tiny menu-bar / system-tray utility that answers one question:

> **How much usage do I have left across my AI coding tools, and when does it reset?**

LimitBar sits in the macOS menu bar (Windows tray support planned), polls each
provider a few times an hour, and shows remaining usage and the next reset time.
No accounts, no cloud, no telemetry. Your provider credentials never leave your machine.

> **Status: early development.** Nothing is packaged or released yet. Expect churn.

## Providers

| Provider     | Status                                                       |
|--------------|--------------------------------------------------------------|
| Command Code | Working — 5-hour + weekly windows, reset times, plan, credits |
| Claude Code  | Investigating                                                |
| OpenAI Codex | Investigating                                                |
| Gemini CLI   | Investigating                                                |

Each provider is investigated before it's implemented. Only sources that give
**authoritative** numbers ship — LimitBar never shows an estimated percentage as if it
were real. See [`docs/provider-feasibility.md`](docs/provider-feasibility.md) for what
data each provider exposes, where it comes from, and the limitations.

## Principles

- **Local-first.** Provider → LimitBar process → UI. There is no LimitBar server.
- **Honest numbers.** If a trustworthy percentage isn't available, the UI says
  "Usage unavailable" rather than inventing one.
- **Invisible when idle.** Tauri 2 + Rust; no Electron. ~0% idle CPU, 5-minute polling,
  countdowns tick locally.
- **One provider failing never breaks the app.** Each provider has independent state,
  caching, and backoff. Stale data stays visible and is labelled stale.
- **Good citizen.** Honest `User-Agent`, low poll rate, honours `429`/`Retry-After`,
  read-only calls only.

## Development

Requirements: Rust (stable), Node 22+, pnpm, and the
[Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```sh
pnpm install
pnpm tauri dev          # run the app
cd src-tauri && cargo test   # Rust unit tests (fixtures only, no credentials needed)
pnpm test               # frontend tests
```

Live provider checks are opt-in and use whatever login already exists on your machine:

```sh
cd src-tauri && cargo test --lib live_ -- --ignored --nocapture
```

## Layout

```
src/               React + TypeScript popup UI (Vite)
src-tauri/src/
  providers/       one adapter per provider behind the UsageProvider trait
  usage/           UsageSnapshot model + UsageManager (cache, backoff, scheduling)
  platform/        tray / menu bar, notifications, autostart
  commands/        IPC surface exposed to the popup
src-tauri/fixtures/  recorded provider responses used by tests
docs/              PRD and provider feasibility research
```

## License

MIT
