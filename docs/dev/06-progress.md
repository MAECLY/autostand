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
| F4 | Wiring: gather → compile → render → write → commit, config store, scheduler runtime | in progress |
| F5 | Brand assets + landing page composition | not started |
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

## F4 — Wiring (in progress)

- [ ] `compute_window(F)` — two-business-day window + date list
- [ ] `compute_provenance` — FORBIDDEN / COVERED / SKEW (`docs/specs/anti-backdating.md`)
- [ ] Gather orchestration over the enabled data sources, with the 2700s TTL cache
- [ ] `anti_regression_guard` — skip when FACTS are empty but the last run had repos
- [ ] `dirty_check` + `persist_hash` — skip re-render when inputs are unchanged
- [ ] LLM render orchestration: provider selection, keychain keys, CLI-first → API, `AUTOSTAND_RENDER=1`
- [ ] `validate_render` — reject bodies that invent tickets, then fall back to deterministic
- [ ] `trigger` / `self_heal` / `commit_push` / `git_sync_pull` with the run lock
- [ ] In-process scheduler runtime emitting `scheduler-tick`, persisted cron
- [ ] `compile_standup`, `compile_all`, `trigger_run_now`, `preview_gather` backed by the real pipeline
- [ ] `pipeline-done` and `scheduler-tick` events actually emitted (their listeners already exist)

## F5 — Brand + landing

- [ ] Full logo set (mark exists; wordmark, lockup, favicon variants missing)
- [ ] Landing page composition reusing the component kit (`docs/design-system/06-landing-reuse.md`)

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
| Audit sidecar per render | `[~]` | writer done (`core::audit`); not yet produced by a real run |
| Anti-backdating (FORBIDDEN/COVERED/SKEW/CLAIM) | `[~]` | `core::scrub` consumes them; nothing computes provenance yet — F4 |
| Union merge driver (`.gitattributes`) | `[ ]` | F4 |
| No-coauthor commits | `[ ]` | F4 (`commit_push`) |
| Self-heal missed runs | `[~]` | `scheduler::selfheal::compute_targets` only; the heal itself is F4 |
