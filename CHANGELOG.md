# Changelog

All notable changes to autostand are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- `MAECLY/autostand-ui` is public, so `@autostand/ui` installs with no
  credential anywhere: pnpm resolves the specifier to a `codeload.github.com`
  tarball over anonymous HTTPS. The read-only deploy key and the token gates
  built around it are on their way out.

### Fixed

- Settings → Local AI showed an empty panel in the published screenshots because
  the marketing capture fired before the model catalog arrived and before the
  tab finished switching.

## [1.0.0] - 2026-08-14

First release. autostand ports the `~/Sync/Github_Dailies` App Script to a desktop app: it reads
what you actually did from eight read-only sources, renders a standup through the AI provider of
your choice, and files it in the same AUTO/MANUAL Markdown format the script always used.

### Added

- **Standup pipeline** — gather → scrub → render → accumulate → redact → atomic write → audit
  sidecar, over the AUTO/MANUAL block format. Every invariant of the App Script is preserved:
  stable host slug, business-day math, anti-backdating, accumulate-never-delete, union-merge
  `.gitattributes` self-heal, write-then-rename + fsync, no-coauthor commits, and a deterministic
  render always computed as the fallback.
- **Eight read-only activity sources** — local-git (authoritative, always on), github via the `gh`
  CLI, claude-code, remember-plugin, opencode, codex, gemini-cli and grok-cli.
- **Six render providers** — Claude, Ollama, OpenAI/Codex, Gemini and Grok, each CLI-first with an
  API fallback, plus a built-in local provider: a curated GGUF catalog (Gemma 3 1B/4B, Qwen 3.5
  2B/4B) downloaded on demand and run through a process-isolated llama.cpp sidecar. Nothing is
  downloaded or selected without an explicit action in Settings → Local AI.
- **Ordered provider failover** — Settings → Providers keeps an explicit provider order and
  per-provider enablement. A failure is isolated to the provider that produced it (auth, quota,
  missing CLI, unsupported model, timeout, transport, empty body, failed validation) before the
  chain advances; a reported rate limit is retried once when the delay is inside the configured
  ceiling. Exhausting the chain falls back to the deterministic renderer.
- **Provider usage probes for nine providers** — Claude, Codex, Cursor, Copilot, Devin, Grok,
  OpenCode, OpenRouter and Z.ai, each read from the credential that vendor's own tool already wrote,
  strictly read-only. Windows carry a label, unit, amounts, period and pace, so credits, dollar
  balances and "N searches left" are reported as themselves rather than forced into a percentage.
  An unreported value reads *No data*, never `0%`.
- **Quota-aware provider selection** — a status-bar badge, a compile pre-flight that informs without
  ever blocking, and health-aware failover. Only a measured dead end (exhausted, rate limited,
  sign-in required) is skipped, and never a provider whose usage is simply unknown; a skip is
  recorded as a skipped attempt and is not fed back as evidence. A burn-rate projection answers the
  question a percentage alone cannot: 40% left is comfortable four hours into a five-hour window and
  alarming ten minutes in.
- **Scheduler** — cron over the 5-field POSIX subset, run locks and missed-run self-heal, plus real
  OS unit installation (launchd agent, systemd `--user` service + timer, or a `schtasks` job
  translated from the cron), so a closed app still files a standup. The unit drives a headless
  `--compile [--date]` mode against the same pipeline. Settings offers a plain schedule builder
  (time, days, once/hourly) and keeps raw cron under Advanced.
- **Terminal panel** — a VS Code-style bottom panel with push layout, drag-resize and minimize,
  opened from the status bar or automatically on compile. Every process the app starts — git, `gh`,
  provider CLIs, the local sidecar, keychain reads, dependency probes, Homebrew installs, scheduler
  probes and notification transports — goes through one spawner and reports into that panel.
- **Dependency checks with guided remediation** for Repo Sync and Local AI. Each names the missing
  tool for this OS and offers the one next step that fixes it; the command is shown in full before
  it can run, only Homebrew installs are executed by the app, and `sudo apt`, `winget` and
  `gh auth login` are displayed and copyable because they need elevation, UAC or a browser.
- **Sync** — Cloud Sync detects iCloud Drive, OneDrive, Dropbox, Syncthing and Nextcloud roots and
  creates `<root>/autostand`; optional Repo Sync versions that same directory in a private GitHub
  repository.
- **Regeneration with review** — Compile now can produce a fresh candidate in an isolated temporary
  directory and show the current AUTO block, the candidate, and an editable merge before anything is
  replaced. The preview never touches the live file, never commits and never pushes. Applying
  rechecks an opaque 30-minute token, the host and a SHA-256 of the exact live file, then rewrites
  only this host's AUTO block — other hosts and the MANUAL region stay verbatim.
- **13 standup format presets** — Classic Scrum, Four Question, Mad/Sad/Glad, Start/Stop/Continue,
  Keep/Drop/Create, Five Question, Spotify 4Q, Async Status, Walking Timebox, Walk the Board,
  Y/T/B/R, Decisions & Commitments and OKR-tied — with verbosity, conventional-commit prefixes and
  optional PR-review / confidence / risk sections, previewed live against a mock standup.
- **Native notifications** — off by default behind a master opt-in that is separate from the OS
  permission, with categories for low usage (with threshold), provider exhausted, fallback used,
  local model downloads, and standup complete or failed.
- **History calendar** — list, month, week, day and agenda views over a single directory read, with
  a shared date picker so History, Audit and Debug stay on the same filing day.
- **Configurable filing date** — Settings → Paths → Filing date chooses whether today's work is
  filed under the next business day's standup (the App Script's rule, and the default) or under
  today's. The Dashboard now states which day is being reported and which file it lands in, because
  they are not the same thing, and Settings says plainly that weekend work accumulates into Monday
  either way.
- **Cold-start readiness** — an editable author list that offers the machine's git identity in one
  click, and a banner naming what is still missing (no scan root, no repos, no authors) instead of
  letting the app file an empty standup in silence.
- **Render provenance** — the Dashboard says which provider and model produced today's standup. The
  written Markdown still names neither, pinned by an end-to-end test.
- **Advanced tab** — Audit and Debug are diagnostic screens, so each now sits behind its own switch,
  off by default; the routes stay registered, so a bookmark still reaches them. Theme and both gates
  persist across launches, and Settings remembers the open tab across navigation.
- **Smaller things that come up daily** — copy buttons on the standup, each AUTO block and the
  MANUAL region; an *open in file manager* button next to every path you can configure; and an
  unload action for the local runtime that kills orphaned llama.cpp processes, purges the prompt/KV
  caches and reports what it freed.
- **Documentation** — 51 files under `docs/`, indexed by `docs/README.md`: architecture, the Tauri
  setup and IPC contract, the LLM adapters, the eight data sources, and the file-format,
  configuration, pipeline, provider-usage, anti-backdating and audit specs, plus dev and user guides.
- **Tests** — over 1,100 Rust tests, over 300 frontend tests and 39 Playwright specs, including a
  hermetic end-to-end pipeline run against a real temporary git repository and a live provider ×
  preset matrix that stays behind an opt-in feature flag so no pull request spends API quota.

### Changed

- The design system moved to [`MAECLY/autostand-ui`](https://github.com/MAECLY/autostand-ui) and is
  consumed here as the private dependency `@autostand/ui`; the marketing site moved to
  [`MAECLY/autostand-landing-page`](https://github.com/MAECLY/autostand-landing-page) and hosting
  moved to Vercel.
- Codex usage is read over HTTP against the credential the `codex` CLI already wrote, instead of
  spawning `codex app-server --stdio` and running a JSON-RPC handshake: no child process, no
  eight-second timeout, and it works whether or not the CLI is on `PATH`.
- CLI detection no longer spawns anything. It walks `PATH`, and the first real call reports its own
  outcome — which also keeps a `gh --version` from blocking inside an async gather.
- Settings → Providers was rebuilt around a single alignment spine: a collapsed card now reports its
  mode and model, reordering lives in the header, and the usage rail renders the plan, notice, stale
  reading and per-window amounts it had been dropping. Pace is printed as well as coloured, so
  colour is never the only channel.
- The Dashboard is organized into Today, Manual item and Pipeline tabs, and keeps showing the
  previous standup while a fresh one loads.
- Pull requests now run only the fast checks. The Playwright suite and the live provider matrix moved
  behind `workflow_dispatch`, so no pull request downloads a browser or spends real provider quota,
  and the Rust job was split into parallel lint, test and audit jobs.
- `make` is the front door to the toolchain; `make check` runs exactly what CI runs.

### Fixed

- **Weekend work no longer disappears.** The compile window was computed two business days back and
  shifted a day into the past, so no file's range ever contained a Saturday or Sunday and that work
  landed in no standup at all — while Thursday was reported twice. Friday, Saturday and Sunday now
  accumulate into Monday's file, and the missing clamp is back, so a filing date can no longer claim
  a full day that has not happened yet. A regression test replays real headers transcribed from the
  original repository byte for byte, because an off-by-one window still produces a plausible-looking
  header.
- **The render prompt no longer comes back as work you did.** The provider CLIs autostand drives log
  their invocation into their own session files, which the data sources then read back, so the
  prompt returned on the next run as your work — section headings, context labels and preset
  skeleton included. The process-level anti-recursion guard could not catch this; a message-level and
  line-level filter now recognises autostand's own prompt at gather time.
- **local-git no longer goes quiet.** The author list is empty on a fresh install and had no control
  anywhere in the UI, so the authoritative source skipped `git log` entirely and returned an empty
  result indistinguishable from "you did no work" — which also left the two strongest render
  validations inert. Authors now resolve from config, then the machine's git identity, then a
  visible error; it never falls back to an unfiltered log that would report your colleagues' commits
  as yours. Every remaining silent path in that source surfaces as a gather failure.
- **A shipped fix now actually recompiles the day.** The unchanged check hashed only the gathered
  inputs, so improving the prompt, a preset or the sanitizer left every already-compiled day marked
  unchanged: the fix was installed and Compile now still skipped it.
- **Compile now reports what it is doing.** It ran without a handle to emit on, so every progress and
  log line of that run was dropped and the panel never opened.
- **Backticks, `[end of text]` markers and document skeletons no longer reach committed standups.**
  llama.cpp prints its end-of-generation marker to the same stream as the tokens, and was
  reinterpreting backslash sequences inside gathered notes and code. A deterministic, idempotent
  sanitizer now repairs mechanical damage before validation — stray fences, duplicated sections,
  unbalanced inline code — while anything semantic still falls back to the deterministic renderer
  rather than being silently rewritten. The output prompt also stopped wrapping its own examples in a
  code fence while instructing the model not to use one.
- **Gemma models answer instead of continuing your document.** Gemma has no system role, and
  autostand was emitting two consecutive user turns, which put the model out of distribution: a 4B
  started restating the file title, subtitle and AUTO markers as content.
- **Grok renders instead of hanging.** A positional `grok "<prompt>"` opened the interactive Build
  UI and sat there until the 180-second timeout, which is why compiles stalled and then fell back to
  the deterministic renderer. Official renders now run headless.
- **Claude Code sessions no longer pad the standup** with unified diffs, changed-file lists and the
  same multi-line preamble repeated; only the human intent survives, deduplicated line by line.
- **Opening Settings no longer probes every provider.** Reading provider health delegated to the
  refresher, so a cache miss ran six sequential probes — one of them an eight-second process spawn —
  and delivered low-usage notifications as a side effect of a render. Reading is now a pure read of a
  disk-backed cache; refreshing is explicit, concurrent, and available per row.
- **A crash no longer wedges the scheduler.** Run locks held by dead processes are reclaimed, and
  locks are released on drop.
- Long paths, commit subjects and previews no longer push content below the fold; the standup
  surfaces and the history rail scroll within their own bounds at every breakpoint.

### Security

- **Third-party credentials are read, never written.** The nine usage probes read the login each
  vendor's own CLI already stored. A refresh token is not even deserialized, an expired access token
  reports *sign-in required* rather than being renewed, and the OS keychain is consulted only on a
  manual refresh, so a background pass can never raise a system dialog. The only thing retained from
  a token is a SHA-256 fingerprint used as a cache key. The Copilot probe filters to `github.com`
  entries, so an Enterprise token is never sent to `api.github.com`.
- **Nothing a subprocess says reaches the interface.** Argv, cwd, env, stdin, stdout and stderr are
  never logged: no heuristic reliably separates a safe subcommand from a customer's branch name, so
  callers supply a display label and the default is the program's file name — never its path, which
  on macOS carries `$HOME`. `brew install` is the single exception that echoes stdout, because a
  silent panel during a multi-minute download is indistinguishable from a hang; its stderr is still
  dropped. A structural test pins that no IPC payload can carry subprocess output.
- **API keys live in the OS keychain**, never in `config.json` and never in a log, and secrets
  redaction runs both before the LLM sees the prompt and before anything is written to disk.
- **Local model downloads are pinned end to end.** Only a catalog id crosses IPC — never a URL or a
  destination path — and each entry pins an immutable Hugging Face revision, an exact byte count and
  a SHA-256 that is verified before the file is installed. Local inference opens no listening socket,
  and chat control markers appearing inside gathered notes are neutralised before they reach the
  model.
- **`AUTOSTAND_RENDER=1` is set on every child process**, including CLI version probes and the local
  sidecar, so a provider CLI cannot re-enter autostand and recurse.
- **CI and the release workflow authenticate with a read-only deploy key**, not a personal access
  token: scoped to a single repository, no write access, tied to no person's account, and revocable
  from that repository's own settings.
- **Release bundles are unsigned.** The codesigning and notarization secrets are not configured, so
  macOS quarantines the download and Gatekeeper usually reports that the app *"is damaged and can't
  be opened"* — misleading wording for a build that is intact but carries no Developer ID signature.
  The README section **"Blocked by Gatekeeper on macOS?"** gives both ways past it and states plainly
  what clearing the quarantine flag gives up, since that check is what protects you from a tampered
  download. Windows bundles are unsigned for the same reason. Signed, notarized builds ship as soon
  as the secrets are set.

### Distribution

- Three installers, one per platform, attached to the tag: a macOS `.dmg`, a
  Windows `-setup.exe` and a Linux `.AppImage`. Each carries its own inference
  sidecar and a pinned `llama-completion`, so built-in local AI works with no
  Ollama, Homebrew, CUDA or system llama.cpp.
- The macOS bundle is **Apple Silicon only** — `macos-13` was dropped from the
  release matrix. Intel Macs run the arm64 build through Rosetta 2.
- The Linux AppImage is built on `ubuntu-22.04`, so it needs **x86_64 and glibc
  ≥ 2.35**: Ubuntu 22.04+, Debian 12+, Fedora 36+, Arch and openSUSE Tumbleweed
  are fine; RHEL 9 and its rebuilds (glibc 2.34) and anything on musl are not.
  AppImages self-mount through FUSE 2 — on a FUSE-3-only distro, run it with
  `--appimage-extract-and-run`.

### Notes

- Auto-update is **not** enabled: the app ships no updater plugin and no `plugins.updater`
  configuration, so releases produce no `latest.json` or `.sig` artifacts and the app never checks
  for updates. Move to a newer version by downloading it. See `docs/dev/04-ci-cd.md` § Tauri updater.
- Remaining gaps are tracked in `docs/dev/06-progress.md`.

[Unreleased]: https://github.com/MAECLY/autostand/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/MAECLY/autostand/releases/tag/v1.0.0
