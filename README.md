# autostand

Cross-platform daily standup automation. A Tauri v2 desktop app (Rust + React) that gathers your work
activity from multiple data sources, renders prose through pluggable AI providers, and writes structured
Markdown standup files.

🚧 In development (v0.1.0, unreleased). Full architecture and specs live in [`docs/`](docs/README.md);
implementation status is tracked in [`docs/dev/06-progress.md`](docs/dev/06-progress.md).

## Where everything lives

autostand is three repositories. **This one is the product**: the Rust workspace and the Tauri desktop app.

| Repo | What | Ships as |
| --- | --- | --- |
| `MAECLY/autostand` (here) | Rust crates + the Tauri v2 desktop app | desktop bundles, from a `v*` tag |
| [`MAECLY/autostand-ui`](https://github.com/MAECLY/autostand-ui) | The design system: tokens, 24 base components, icons, brand fonts, Storybook | the `@autostand/ui` package, consumed here as a git dependency |
| [`MAECLY/autostand-landing-page`](https://github.com/MAECLY/autostand-landing-page) | The marketing site (Next.js 15) | a Vercel deployment |

The design system is a private git dependency (`"@autostand/ui": "github:MAECLY/autostand-ui#main"`), pinned by
commit in `pnpm-lock.yaml`. Installing it needs read access to that repo — an SSH key locally, and the
`AUTOSTAND_UI_TOKEN` secret in CI. See [`docs/dev/04-ci-cd.md`](docs/dev/04-ci-cd.md) § Private dependency
authentication.

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
- **Design system**: Tailwind v4 tokens + shadcn/ui + Storybook 8, shared with the marketing site through the
  `@autostand/ui` package.

## Prerequisites

| Tool | Version | Notes |
| --- | --- | --- |
| Rust | stable (workspace MSRV 1.80) | `rustup toolchain install stable` |
| Node | ≥ 22.13 | pnpm 11.18 refuses to run below it; the old floor of 20 was wrong and broke the release build |
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
pnpm build:web              # the app's web bundle (Vite), the only web surface here
pnpm build                  # production desktop bundles (tauri build)
```

## Install

Grab the installer for your platform from the
[latest release](https://github.com/MAECLY/autostand/releases/latest) — one per
platform, no build required.

| Platform | File | Notes |
| --- | --- | --- |
| macOS | `.dmg` | Apple Silicon. Intel Macs run it through Rosetta 2. |
| Windows | `-setup.exe` | |
| Linux | `.AppImage` | `chmod +x` it and run — no package manager involved. |

Every bundle carries its own inference sidecar and a pinned `llama-cli`, so
Built-in Local AI works without Ollama, Homebrew, CUDA or a system llama.cpp.

macOS users: read the next section first, or the app will appear to be broken.

## Blocked by Gatekeeper on macOS?

Released builds are **unsigned**: the codesigning secrets in `release.yml` are
not configured, so `tauri-action` produces an unsigned bundle. macOS therefore
quarantines the download and Gatekeeper refuses to launch it, usually reporting
that the app *"is damaged and can't be opened"* — which is misleading. The
download is intact; it simply carries no Developer ID signature.

To clear the quarantine flag:

1. Open **Terminal** — press `Cmd + Space`, type `Terminal`, press Enter.
2. Run:

   ```bash
   xattr -rd com.apple.quarantine /Applications/autostand.app
   ```

3. Launch autostand normally.

There is also a GUI path: try to open the app once, then go to **System
Settings → Privacy & Security** and choose **Open Anyway**. Right-click → Open
no longer works for this case on recent macOS.

Run this only on a build you fetched from
[this repository's releases](https://github.com/MAECLY/autostand/releases).
Stripping the quarantine flag is exactly the check that protects you from a
tampered download, so it is worth knowing what you are turning off. Once the
Developer ID secrets are set, notarized builds launch without any of this.

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

# End-to-end (Playwright)
pnpm test:e2e                               # app UI over a mocked Tauri IPC
```

`pnpm test:e2e` needs a Chromium build once: `pnpm --filter autostand-app exec playwright install chromium`.

Everything above is what CI runs. The Rust tests are hermetic — no network,
no real `git`/`gh`/LLM calls, and independent of your `HOME`.

## Repository layout

| Path | What |
| --- | --- |
| `crates/autostand-core` | Domain model, file format, business rules, deterministic renderer, audit |
| `crates/autostand-adapters` | `LlmAdapter` + 5 providers; `DataSource` + 8 sources |
| `crates/autostand-scheduler` | Cron parser, self-heal, OS scheduler installation |
| `apps/autostand-app` | The Tauri app — Rust in `src-tauri/`, React/Vite in `src/` |
| `brand` | Logo SVGs and the Open Graph card; the generators live in `tests/` |
| `tests/e2e` | Playwright specs for the app UI |
| `docs` | Full project documentation — start at [`docs/README.md`](docs/README.md) |

## Published sites

None from this repo. It used to publish the landing page and Storybook as one GitHub Pages site; both moved to
their own repositories in the split, and `pages.yml` is gone. The marketing site deploys to Vercel from
`autostand-landing-page`, and Storybook builds in `autostand-ui`.

Desktop bundles are built here: pushing a `v*` tag runs
[`.github/workflows/release.yml`](.github/workflows/release.yml), which attaches macOS (Apple Silicon +
Intel), Linux and Windows bundles to a draft release for a human to publish. Codesigning secrets are not
configured yet, so those bundles are unsigned — see [`docs/user/01-install.md`](docs/user/01-install.md).

## Contributing

Conventions, invariants and the agent workflow are in [`AGENTS.md`](AGENTS.md). Conventional commits; docs
are updated alongside code.

## License

MIT
