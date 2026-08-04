# Progress tracker

Implementation status of autostand, phase by phase. **This file is the tracker** — update it at the end of every
phase, in the same commit range as the work. Every claim here must be backed by code that builds and tests that
pass; a box is only checked when `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
`pnpm typecheck`, `pnpm lint` and `pnpm test` are green with the feature in place.

Legend: `[x]` done and verified · `[~]` partially done (detail in the note) · `[ ]` not started.

---

## Phases

| Phase | Scope | Status |
| --- | --- | --- |
| F0 | Documentation (47 files under `docs/`) | done |
| F1 | Scaffolding: monorepo, Tauri v2 shell, design-system skeleton | done |
| F2 | Core domain + adapters (8 data sources, 5 LLM providers, pipeline, renderer, audit) | done |
| F3 | Frontend + IPC contract (25 commands, 23 base components, 9 hooks, 5 pages) | done |
| F4 | Wiring: gather → compile → render → write → commit, config store, scheduler runtime | done |
| F5 | Brand assets + landing page composition | done |
| F6 | E2E tests + CI/CD | not started |

---

## F0 — Documentation

- [x] Architecture (overview, monorepo, data flow, state machine, security)
- [x] Tauri (setup, IPC contracts, platform targets, frontend stack)
- [x] LLM adapters (5 providers, trait, render prompt)
- [x] Data sources (8 sources, trait)
- [x] Specs (file format, configuration, pipeline, anti-backdating, audit)
- [x] Dev + user guides, design system

## F1 — Scaffolding

- [x] Cargo workspace with shared lints (`unsafe_code = deny`, clippy pedantic)
- [x] Tauri v2 app shell, capabilities, icons
- [x] pnpm workspace: `apps/autostand-app` + `design-system`
- [x] Design tokens (`design-system/tokens/tokens.css`) + Storybook 8 config

## F2 — Core domain + adapters

- [x] Domain model + AUTO/MANUAL parser and writer (`format`, `standup`, `fileops`)
- [x] Business-day math (`dates`), host slug (`host`), redaction (`redact`), similarity (`textsim`)
- [x] Deterministic renderer (`deterministic`), accumulate (`accumulate`), audit sidecar (`audit`)
- [x] Pure-sync `pipeline::compile_file` (scrub → render → accumulate → redact → write → audit)
- [x] 8 data sources implementing `DataSource`
- [x] 5 LLM providers implementing `LlmAdapter` (CLI + API)
- [x] Cron parser + `next_run` (5-field POSIX subset)

## F3 — Frontend + IPC

- [x] 25 IPC commands matching `docs/tauri/02-ipc-contracts.md`, `AppError`, managed `AppState`
- [x] Wire format verified against the contract (serde renames + round-trip tests)
- [x] 23 design-system base components + Storybook stories
- [x] Shared Tailwind v4 stylesheet consumed by app and Storybook
- [x] TS contract layer (`lib/types.ts`, `lib/tauri.ts`, `lib/error.ts`, `lib/store.ts`)
- [x] 9 TanStack Query hooks, app shell, standup/settings/audit/debug components
- [x] 5 pages (dashboard, settings, history, audit, debug)
- [x] 176 Rust tests · 97 frontend tests

## F4 — Wiring

- [x] `compute_window(F)` — two-business-day window + date list (`core::dates`)
- [x] `compute_provenance` — FORBIDDEN / COVERED / SKEW (`core::provenance`)
- [x] Gather orchestration over the enabled data sources, with the 2700s TTL cache (`app::gather`)
- [x] `anti_regression_guard` — skip when FACTS are empty but the last run had repos
- [x] `dirty_check` + `persist_hash` — sha256 over the redacted inputs (`core::hashes`)
- [x] LLM render orchestration: provider selection, keychain keys, CLI-first → API, `AUTOSTAND_RENDER=1`
- [x] `validate_render` — reject bodies that invent tickets, then fall back to deterministic
- [x] `trigger` / `self_heal` / `commit_push` / `git_sync_pull` with the run lock (`app::pipeline_runner`)
- [x] In-process scheduler runtime emitting `scheduler-tick`, persisted cron
- [x] `compile_standup`, `compile_all`, `trigger_run_now`, `preview_gather`, `test_llm_provider` backed by real code
- [x] `pipeline-done` and `scheduler-tick` events actually emitted
- [x] End-to-end integration tests over a real temp git repo (`tests/pipeline_e2e.rs`)
- [ ] System scheduler installation (launchd / systemd / Task Scheduler) — `SchedulerStatus.source` is always
      `in-process` today; installing the OS unit is deferred to F6 alongside packaging

Test totals after F4: **411 Rust tests** (was 176) · 97 frontend tests.

## F5 — Brand + landing

- [x] Full logo set — mark, horizontal, vertical, mono, favicon; wordmark as Inter 700 path outlines
- [x] Real app icons (32/128/128@2x/512/icon.png, a multi-size `.ico`, `.icns`) + the 1200×630 Open Graph card
- [x] Self-hosted Inter + JetBrains Mono woff2, wired through the shared stylesheet and bundled by the app
- [x] Custom icon set (`design-system/icons/`) with React exports and stories
- [x] Landing page (`apps/landing/`, Astro static + React islands) reusing the tokens and base components
- [x] Dark mode, skip link, section anchors; only the theme toggle and FAQ ship JavaScript
- [ ] Deploy the landing page (the build is static and GitHub-Pages-ready under `/autostand`; no workflow yet — F6)

## F6 — E2E + CI/CD

- [ ] Playwright E2E suite (`tests/e2e/`)
- [ ] GitHub Actions: lint, test, build, release (`docs/dev/04-ci-cd.md`) — no `.github/` yet

---

## App Script invariants

These are the behaviours the port must preserve, from `~/Sync/Github_Dailies`. The list in
`docs/dev/05-migration-from-appscript.md` states the **requirement**; this table states the **implementation
status**.

| Invariant | Status | Where |
| --- | --- | --- |
| Host slug stability (persisted, rejects numeric/IP) | `[x]` | `core::host` |
| `next_business_day` / `prev_business_day_before` | `[x]` | `core::dates` |
| AUTO/MANUAL block structure | `[x]` | `core::format`, `core::fileops` |
| Atomic write-then-rename + fsync | `[x]` | `core::fileops::write_atomic` |
| Accumulate never-delete | `[x]` | `core::accumulate` |
| Deterministic fallback always computed | `[x]` | `core::deterministic` |
| Secrets redaction pre-LLM and pre-write | `[x]` | `core::redact` |
| Audit sidecar per render | `[x]` | `core::audit`; asserted end-to-end by `pipeline_e2e` |
| Anti-backdating (FORBIDDEN/COVERED/SKEW/CLAIM) | `[x]` | `core::provenance` + `core::scrub`; asserted end-to-end |
| Union merge driver (`.gitattributes`) | `[x]` | `app::git_ops::ensure_gitattributes` (idempotent self-heal) |
| No-coauthor commits | `[x]` | `app::git_ops::commit_push`; pinned by a test that greps the commit |
| Self-heal missed runs | `[x]` | `app::pipeline_runner` + `scheduler::selfheal::is_frozen` |
