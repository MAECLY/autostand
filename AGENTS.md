# AGENTS.md

## Project

`autostand` — cross-platform daily standup automation (Tauri v2 + Rust workspace + React/Vite/Tailwind v4/shadcn).

Three repos: **this one** is the product. The design system is
[`MAECLY/autostand-ui`](https://github.com/MAECLY/autostand-ui), consumed here as the private git dependency
`@autostand/ui`; the marketing site is
[`MAECLY/autostand-landing-page`](https://github.com/MAECLY/autostand-landing-page). Do not add a design-system
directory back to this repo.

Ports the `~/Sync/Github_Dailies` Bash/Python "App Script" to a desktop app with pluggable AI providers and data sources. The original App Script must NOT be modified.

## Repository layout

- `crates/autostand-core` — domain model, file format, business rules (anti-backdating, accumulate, scrub, redact, deterministic renderer).
- `crates/autostand-adapters` — `LlmAdapter` trait + 5 providers (Claude, Ollama, OpenAI/Codex, Gemini, Grok) × CLI+API; `DataSource` trait + 8 sources.
- `crates/autostand-scheduler` — cron, triggers, locks, self-heal.
- `apps/autostand-app/` — Tauri v2 app (Rust `src-tauri/` + React/Vite `src/`).
- `brand/` — logo SVG variants, typography, palette.
- `docs/` — full project documentation (47 files, see `docs/README.md`).
- `tests/` — `unit/`, `integration/`, `e2e/`. Throwaway scratch scripts also go here.

## Build / test commands

```bash
# Rust
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
cargo audit

# Frontend
pnpm install
pnpm tauri dev          # hot-reload dev app
pnpm tauri build        # production bundles
pnpm build:frontend     # frontend only
pnpm lint
pnpm typecheck
pnpm test               # vitest
pnpm test:e2e           # Playwright

# Design system — in the autostand-ui repo, not here.
# Here it is just a dependency, pinned by commit in pnpm-lock.yaml:
pnpm --filter autostand-app update @autostand/ui
```

`make` lists every target and is the front door to all of the above.

## Conventions

- **Rust**: edition 2021, rust-version 1.80. Workspace lints: `unsafe_code = "deny"`, clippy `all` + `pedantic` + `cargo` warn. Use `anyhow` for app errors, `thiserror` for library errors. `#[tracing]` for instrumented logs.
- **Frontend**: React + TypeScript strict. Tailwind v4 via `@tailwindcss/vite` (no `tailwind.config.js` — use `@theme` in CSS). Base components and tokens come from `@autostand/ui` — import them by subpath (`@autostand/ui/components/button`, `@autostand/ui/icons`, `@autostand/ui/lib/utils`), never by relative path, and never copy one into `src/`.
- **Commits**: conventional commits (`feat:`, `fix:`, `docs:`, `chore:`, `test:`). No coauthor trailers. No `--no-verify`.
- **Tests**: unit tests in `#[cfg(test)] mod tests { ... }` inside each Rust module; integration in `tests/integration/`; E2E in `tests/e2e/` via Playwright. Hermetic temp HOME for pipeline tests.
- **Secrets**: NEVER hardcode API keys, tokens, passwords. API keys go in OS keychain (`keyring` crate), never in config JSON, never logged. Secrets redaction runs pre-LLM and pre-write.
- **Docs**: all design decisions documented in `docs/`. Update docs alongside code.

## AI providers (5)

Claude (Anthropic) — CLI `claude -p` or API.
Ollama — CLI `ollama run` or local API `http://localhost:11434`.
OpenAI/Codex — Codex CLI or OpenAI API.
Gemini — Gemini CLI or Google API.
Grok — Grok CLI (auto-detect variant) or xAI API.

All providers: CLI-first → API fallback (configurable per provider). Anti-recursion env `AUTOSTAND_RENDER=1` set on CLI subprocess calls.

## Data sources (8)

local-git (authoritative, always on), github (gh CLI), claude-code, remember-plugin, opencode (SQLite + JSON), codex, gemini-cli, grok-cli. All read-only.

## Invariants (must preserve from App Script)

Host slug stability (persist, no DHCP); `next_business_day`/`prev_business_day_before`; AUTO/MANUAL block structure; union merge driver; atomic write-then-rename+fsync; anti-backdating; accumulate-never-delete; deterministic fallback always computed; secrets redaction; audit sidecar per render; no-coauthor commits; self-heal missed runs.