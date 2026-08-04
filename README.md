# autostand

Cross-platform daily standup automation. A Tauri v2 desktop app (Rust + React) that gathers your work
activity from multiple data sources, renders prose through pluggable AI providers, and writes structured
Markdown standup files.

🚧 In development (v0.1.0, unreleased). Full architecture and specs live in [`docs/`](docs/README.md);
implementation status is tracked in [`docs/dev/06-progress.md`](docs/dev/06-progress.md).

## Features

- **Cross-platform**: macOS, Linux, Windows (Tauri v2).
- **5 AI providers**: Claude, Ollama, OpenAI/Codex, Gemini, Grok — CLI-first with API fallback.
- **8 data sources**: local git, GitHub (`gh` CLI), Claude Code sessions, Remember plugin, OpenCode, Codex,
  Gemini CLI, Grok CLI. All read-only.
- **Anti-backdating**: git owns committed work; notes are scrubbed; phantoms are caught in an audit sidecar.
- **Accumulate-never-delete**: previous bullets are re-injected if a new render does not cover them.
- **Two-machine sync**: per-host AUTO blocks plus a union merge driver.
- **Scheduled + self-healing**: installs a launchd agent / systemd `--user` timer / Task Scheduler job and
  fills missed runs from durable on-disk data.
- **Design system**: Tailwind v4 tokens + shadcn/ui + Storybook 8, shared by the app and the landing page.

## Prerequisites

| Tool | Version | Notes |
| --- | --- | --- |
| Rust | stable (workspace MSRV 1.80) | `rustup toolchain install stable` |
| Node | ≥ 20 (CI uses 22) | one dev dependency wants ≥ 22, so 22 avoids an engine warning |
| pnpm | 11.18.0 | pinned via `packageManager`; `corepack enable` picks it up automatically |

Linux additionally needs the Tauri v2 system libraries (see the `Install Tauri v2 Linux system dependencies`
step in [`.github/workflows/ci.yml`](.github/workflows/ci.yml) for the exact `apt` list).

Rendering prose needs at least one LLM CLI or API key, but the app always computes a deterministic fallback,
so it is usable with none.

## Quick start

```bash
git clone https://github.com/MAECLY/autostand.git
cd autostand
pnpm install
pnpm dev            # hot-reload desktop app (tauri dev)
```

## Build

```bash
cargo build --workspace     # Rust crates only
pnpm build:web              # the three web surfaces: app, landing page, Storybook
pnpm build                  # production desktop bundles (tauri build)
```

## Test and lint

```bash
# Rust
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit

# Frontend
pnpm lint
pnpm typecheck
pnpm test                                   # vitest

# End-to-end (Playwright; not yet part of CI)
pnpm test:e2e                               # app UI over a mocked Tauri IPC
pnpm --filter landing test:e2e              # the built landing page
```

`pnpm test:e2e` needs a Chromium build once: `pnpm --filter autostand-app exec playwright install chromium`.

Everything above is what CI runs, minus the two Playwright suites. The Rust tests are hermetic — no network,
no real `git`/`gh`/LLM calls, and independent of your `HOME`.

## Repository layout

| Path | What |
| --- | --- |
| `crates/autostand-core` | Domain model, file format, business rules, deterministic renderer, audit |
| `crates/autostand-adapters` | `LlmAdapter` + 5 providers; `DataSource` + 8 sources |
| `crates/autostand-scheduler` | Cron parser, self-heal, OS scheduler installation |
| `apps/autostand-app` | The Tauri app — Rust in `src-tauri/`, React/Vite in `src/` |
| `apps/landing` | Astro marketing site |
| `design-system` | Tokens, 23 base components, icons, Storybook 8 |
| `tests/e2e` | Playwright specs for the app UI |
| `docs` | Full project documentation — start at [`docs/README.md`](docs/README.md) |

## Published sites

Both are deployed from `main` by [`.github/workflows/pages.yml`](.github/workflows/pages.yml) as a single
GitHub Pages site:

- Landing page — <https://maecly.github.io/autostand/>
- Storybook — <https://maecly.github.io/autostand/storybook/>

Run Storybook locally with `pnpm storybook`.

Desktop bundles are built separately: pushing a `v*` tag runs
[`.github/workflows/release.yml`](.github/workflows/release.yml), which attaches macOS (Apple Silicon +
Intel), Linux and Windows bundles to a draft release for a human to publish. Codesigning secrets are not
configured yet, so those bundles are unsigned — see [`docs/user/01-install.md`](docs/user/01-install.md).

## Contributing

Conventions, invariants and the agent workflow are in [`AGENTS.md`](AGENTS.md). Conventional commits; docs
are updated alongside code.

## License

MIT
