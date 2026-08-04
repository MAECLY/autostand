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
| F0 | Documentation (48 files under `docs/`) | done |
| F1 | Scaffolding: monorepo, Tauri v2 shell, design-system skeleton | done |
| F2 | Core domain + adapters (8 data sources, 5 LLM providers, pipeline, renderer, audit) | done |
| F3 | Frontend + IPC contract (25 commands, 23 base components, 9 hooks, 5 pages) | done |
| F4 | Wiring: gather → compile → render → write → commit, config store, scheduler runtime | done |
| F5 | Brand assets + landing page composition | done |
| F6 | E2E tests + CI/CD | done |

---

## Last full verification

Every command below was run locally, in this repo, at the close of F6 — the whole of `ci.yml`, including the
E2E job.

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --workspace` | **470 passed**, 0 failed, 0 ignored |
| `cargo audit` | pass — 0 vulnerabilities, 17 unmaintained/unsound warnings, none suppressed |
| `pnpm install --frozen-lockfile` | pass (lockfile in sync, pnpm 11.18.0) |
| `pnpm lint` | pass (3 packages) |
| `pnpm typecheck` | pass (3 packages, 0 errors) |
| `pnpm test` | **97 passed** (8 files) |
| `pnpm build:web` | pass — app (vite), landing (astro), Storybook |
| `pnpm --filter autostand-app test:e2e` | **20 passed** / 20 |
| `pnpm --filter landing test:e2e` | **46 passed** / 46, axe gate included |

Hermeticity re-checked: `cargo test --workspace` also passes with `HOME` pointed at an empty temp
directory and `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` set to `/dev/null` — no test reads the developer's
home or git identity. No secret, token or key is committed anywhere; the only key-shaped strings in the
tree are the synthetic fixtures in `core::redact`'s own tests.

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
- [x] pnpm workspace: `apps/autostand-app` + `design-system` (`apps/landing` joined it in F5 — 3 packages today)
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
- [x] System scheduler installation (launchd / systemd / Task Scheduler) — **closed in F6**, not F4.
      `scheduler::install` writes the launchd plist / systemd `--user` service+timer / `schtasks` job and
      `scheduler_runtime::resolve_source(install::detect(), …)` now reports the real source instead of
      hard-coding `in-process`. `main.rs` grew the `--compile [--date]` headless entry point the unit runs.
      Install is attempted only from `set_scheduler_schedule`; `AUTOSTAND_NO_INSTALL` and `cfg!(test)` keep
      the unit tests from touching the developer's launchd/systemd.

Test totals after F4: **411 Rust tests** (was 176) · 97 frontend tests.

## F5 — Brand + landing

- [x] Full logo set — mark, horizontal, vertical, mono, favicon; wordmark as Inter 700 path outlines
- [x] Real app icons (32/128/128@2x/512/icon.png, a multi-size `.ico`, `.icns`) + the 1200×630 Open Graph card
- [x] Self-hosted Inter + JetBrains Mono woff2, wired through the shared stylesheet and bundled by the app
- [x] Custom icon set (`design-system/icons/`) with React exports and stories
- [x] Landing page (`apps/landing/`, Astro static + React islands) reusing the tokens and base components
- [x] Dark mode, skip link, section anchors; only the theme toggle and FAQ ship JavaScript
      (all four re-verified green by the F6 landing suite: `theme.spec.ts`, `a11y.spec.ts` skip-link tests,
      `navigation.spec.ts`, `structure.spec.ts` "hydrates both islands and only those")
- [x] Deploy the landing page — `pages.yml` publishes it at `/autostand` alongside Storybook at
      `/autostand/storybook/` (closed in F6)
- [x] **WCAG 2.1 AA colour contrast — met.** F6's axe gate measured the palette for the first time and found
      13 token pairs in `design-system/tokens/tokens.css` below 4.5:1. Root cause was structural, not cosmetic:
      `.dark` had no overrides at all for `--brand-primary`, `--audit-*` or `--status-*-bg`, and text on a
      brand-filled control reused `--fg-inverse`, which flips to slate-950 in dark mode — so every primary
      button label sat at 3.01:1. Fixed by adding a dedicated `--fg-on-brand`, giving dark mode real overrides
      (the brand hue lightens two stops on dark surfaces; `#2563eb` remains the light-mode brand blue and the
      logo assets are untouched), and darkening six light-mode status/audit values one stop. Two markup defects
      went with it: `<Progress>` in the mockup had no accessible name, and the table's `overflow-auto` wrapper
      was unreachable by keyboard.
      Verified: `tests/verify-f5-contrast.py` reports 0 pairs below threshold, and all four axe audits pass.

## F6 — E2E + CI/CD

- [x] GitHub Actions — `ci.yml`, `pages.yml`, `release.yml`. All three parse as YAML; every pinned action ref
      resolves on GitHub (`actions/checkout@v4`, `actions/setup-node@v4`, `actions/configure-pages@v5`,
      `actions/upload-pages-artifact@v3`, `actions/deploy-pages@v4`, `pnpm/action-setup@v4`,
      `Swatinem/rust-cache@v2`, `tauri-apps/tauri-action@v0`, and `dtolnay/rust-toolchain@stable`, which is a
      branch rather than a tag — the documented usage). Permissions are least-privilege per workflow:
      `ci.yml` `contents: read`, `pages.yml` `contents: read` + `pages: write` + `id-token: write`,
      `release.yml` `contents: write`. Secrets appear only as `${{ secrets.* }}`.
- [x] Every `ci.yml` step run locally and green — see the verification table above.
- [x] `pages.yml`'s two inline scripts (`Assemble site`, `Verify base paths`) executed verbatim against real
      builds: the landing page's root-absolute refs are all under `/autostand/` and present in the artifact,
      and Storybook emits a fully relative bundle.
- [x] `release.yml`'s gate executed: `python3 tests/verify-version-consistency.py v0.1.0` agrees across all
      10 manifests.
- [x] Playwright E2E — two suites, 66 specs, **all green**:
      - `tests/e2e/` (app UI over a mocked Tauri IPC, config in `apps/autostand-app/playwright.config.ts`) —
        **20/20**.
      - `apps/landing/e2e/` (the built Astro site served under its deployed base path) — **46/46**, axe gate
        included.
- [x] Both suites run in CI. `ci.yml` has a third job that installs chromium only and runs each suite, with
      failure artifacts uploaded for 7 days.
- [ ] Tauri updater — deliberately not enabled (no updater plugin, no `plugins.updater`, no
      `bundle.createUpdaterArtifacts`), so no `latest.json` and no `.sig` are produced and
      `TAURI_SIGNING_PRIVATE_KEY*` are not wired. `docs/dev/04-ci-cd.md` § Tauri updater lists what turning it
      on needs. Codesigning/notarization secrets are referenced but unset, so bundles ship unsigned.

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

---

## Re-audit of the earlier phases (done at the close of F6)

Every box above was re-counted against the tree as it stands, not trusted from the phase that wrote it.
Nothing regressed; F0–F3 are as claimed:

| Claim | Counted | Verdict |
| --- | --- | --- |
| 25 IPC commands | 25 `#[tauri::command]` in `src-tauri/src/**`, all reachable from `generate_handler!` | holds |
| 23 design-system base components | 23 `.tsx` components, each with a `.stories.tsx` | holds |
| 9 TanStack Query hooks | 9 files in `src/hooks/` | holds |
| 5 pages | `index`, `settings`, `history`, `audit`, `debug` under `src/routes/` | holds |
| 8 data sources | `local_git`, `github`, `claude_code`, `remember`, `opencode`, `codex`, `gemini_cli`, `grok_cli` | holds |
| 5 LLM providers | `claude`, `ollama`, `openai`, `gemini`, `grok` | holds |
| 97 frontend tests | 97 in 8 files | holds |
| 47 docs files | **48** `.md` under `docs/` | corrected above |

## Outstanding for the project

1. **`DataSourceConfig::from_env()`** (`crates/autostand-adapters/src/sources/traits.rs`) still falls back to
   the original author's identity and their employer's hosts — `miguel@fiftyflowers.com|miguel50flowers`,
   `fifty-git`, `https://fiftyflowers.atlassian.net/browse`. It is env-overridable and, importantly, the app
   never calls it: the Tauri app builds its `DataSourceConfig` from the persisted `AppConfig` via
   `gather::source_config`, and `AppConfig`/`ReviewConfig` default every one of those fields to empty. So a
   fresh clone is *not* contaminated — but this is dead public API carrying a personal identity, and
   `docs/specs/configuration.md` still documents `jira_base`'s default as the fiftyflowers URL, which the
   code no longer does.
2. **`docs/dev/03-testing.md` describes an E2E design that was not built.** It says E2E "launch the full
   Tauri app via Playwright + `tauri-driver`". What exists instead mocks the Tauri IPC boundary and drives the
   real frontend in Chromium — a deliberate, documented trade (`tests/e2e/README.md` explains why), but the
   testing doc has not caught up.
3. **Release signing.** macOS notarization and Windows codesigning secrets are referenced in `release.yml`
   but unset, so `v*` tags produce unsigned bundles on a draft release.
