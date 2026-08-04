# Changelog

All notable changes to autostand are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Nothing has shipped yet — the repository has no tags, so everything below sits under
`[Unreleased]`. Cutting `v0.1.0` means moving these entries under a dated heading and
following `docs/dev/04-ci-cd.md` § Manual release process.

## [Unreleased]

### Added

- **Documentation (F0)** — 47 files under `docs/`, indexed by `docs/README.md`: architecture,
  Tauri setup and the frozen IPC contract, the 5 LLM adapters, the 8 data sources, the file-format /
  configuration / pipeline / anti-backdating / audit specs, dev and user guides, and the design system.
- **Workspace scaffolding (F1)** — Cargo workspace with shared lints (`unsafe_code = deny`, clippy
  `all` + `pedantic` + `cargo`), the Tauri v2 app shell with its capability file and icons, a pnpm
  workspace, design tokens in `design-system/tokens/tokens.css`, and Storybook 8.
- **Core domain — `autostand-core` (F2, F4)**
  - AUTO/MANUAL block parser and writer (`format`, `standup`, `fileops`), with atomic
    write-then-rename + fsync.
  - Business-day math (`dates`), stable host slug (`host`), secrets redaction (`redact`),
    fuzzy similarity (`textsim`).
  - Deterministic renderer (`deterministic`), accumulate-never-delete (`accumulate`), audit sidecar
    (`audit`), and the pure-sync `pipeline::compile_file` that chains scrub → render → accumulate →
    redact → write → audit.
  - `compute_window` (two-business-day compile window) and sha256 input hashing (`hashes`).
  - `compute_provenance` — FORBIDDEN / COVERED / SKEW classification (`provenance`).
- **Adapters — `autostand-adapters` (F2)** — 8 read-only data sources (local-git, github, claude-code,
  remember-plugin, opencode, codex, gemini-cli, grok-cli) behind the `DataSource` trait, and 5 LLM
  providers (Claude, Ollama, OpenAI/Codex, Gemini, Grok) behind `LlmAdapter`, each with a CLI path and
  an API path.
- **Scheduler — `autostand-scheduler` (F2)** — cron parser and `next_run` over the 5-field POSIX
  subset, run locks, and missed-run self-heal.
- **Tauri backend (F3, F4)** — the 25 IPC commands from `docs/tauri/02-ipc-contracts.md` with
  `AppError` and a managed `AppState`; gather orchestration across the enabled sources with a 2700 s
  TTL cache; LLM render orchestration (provider selection, keychain-stored keys, CLI-first → API
  fallback, `AUTOSTAND_RENDER=1` anti-recursion guard) with `validate_render` falling back to the
  deterministic renderer; git operations for the dailies repo (union-merge `.gitattributes` self-heal,
  no-coauthor commits, pull/push); `trigger` / `self_heal` / `commit_push` under the run lock; and an
  in-process scheduler runtime emitting `scheduler-tick` and `pipeline-done`.
- **Frontend (F3)** — typed IPC contract layer (`lib/types.ts`, `lib/tauri.ts`, `lib/error.ts`,
  `lib/store.ts`), 9 TanStack Query hooks, the app shell (sidebar, top bar, status bar), and the
  dashboard, settings, history, audit and debug pages.
- **Design system (F3, F5)** — 23 base components on Radix + design tokens with a Storybook story
  each, one Tailwind v4 stylesheet shared by the app and Storybook, a custom icon set with React
  exports, and self-hosted Inter + JetBrains Mono woff2.
- **Brand (F5)** — logo suite (mark, horizontal, vertical, mono, favicon) with the wordmark drawn as
  real path outlines, the real app icon set (32/128/128@2x/512, multi-size `.ico`, `.icns`), and the
  1200×630 Open Graph card.
- **Landing page (F5)** — `apps/landing/`, a static Astro site with React islands that reuses the
  design tokens and base components; dark mode, skip link and section anchors, with JavaScript
  shipped only for the theme toggle and the FAQ.
- **Tests** — 411 Rust tests and 97 frontend tests; hermetic pipeline integration tests including an
  end-to-end run over a real temporary git repository (`tests/pipeline_e2e.rs`); brand, icon,
  contrast and accessibility verification scripts under `tests/`.

### Changed

- `compile_standup` and the other pipeline commands went from stubs to real code backed by the
  gather → compile → render → write → commit path.
- design-system linting no longer walks the Storybook build output.

### Fixed

- IPC wire format aligned with the documented contract (serde renames plus round-trip tests), so the
  frontend types and the Rust DTOs cannot drift.
- Scheduler locks held by dead processes are reclaimed, and locks are released on drop.
- Corrected the `git log` window and the multi-author filter in the local-git data source.
- The anti-recursion guard is now set on CLI version probes too, not only on render calls.
- App icons are generated from the real mark instead of flat blue squares.

### Notes

Known gaps at the time of writing, tracked in `docs/dev/06-progress.md`:

- System scheduler installation (launchd / systemd / Task Scheduler) is not implemented —
  `SchedulerStatus.source` is always `in-process`.
- Auto-update is **not** enabled: the app has no updater plugin and no `plugins.updater`
  configuration, so releases do not produce `latest.json` or `.sig` artifacts. See
  `docs/dev/04-ci-cd.md` § Tauri updater.
- Release bundles are unsigned until the codesigning and notarization secrets are configured.

[Unreleased]: https://github.com/MAECLY/autostand/commits/main
