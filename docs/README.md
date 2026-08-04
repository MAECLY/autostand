# autostand — cross-platform daily standup automation (Tauri v2)

> Ports the Github_Dailies App Script to a desktop app with pluggable AI providers and data sources.

**autostand** gathers work activity from local git, GitHub, and multiple AI assistant
session logs, renders a structured Markdown standup file via a pluggable LLM provider,
and writes it atomically to a `dailies/` directory that syncs across machines.

> ⚠️ **This repo is a NEW implementation.** The original App Script lives at
> `~/Sync/Github_Dailies` and **must NOT be modified**. autostand reimplements the
> App Script's behavior in Rust + React/Tauri for cross-platform support and must
> stay independent of the legacy Bash/Python codebase.

---

## Documentation index

### Architecture

| File | Description |
| --- | --- |
| [architecture/01-overview.md](architecture/01-overview.md) | System overview, design principles, workspace crates, cross-platform notes. |
| [architecture/02-monorepo-structure.md](architecture/02-monorepo-structure.md) | Full directory tree, crate responsibilities, dependency graph. |
| [architecture/03-data-flow.md](architecture/03-data-flow.md) | End-to-end pipeline diagram, data-source priority, cache layer, state files. |
| [architecture/04-state-machine.md](architecture/04-state-machine.md) | File lifecycle, render mode, host slug, lock, and per-run state machines. |
| [architecture/05-security.md](architecture/05-security.md) | Redaction, anti-recursion, atomic writes, keychain, threat model. |

### Tauri & frontend

| File | Description |
| --- | --- |
| [tauri/01-tauri-setup.md](tauri/01-tauri-setup.md) | Tauri v2 project bootstrap, capabilities, permissions, bundling. |
| [tauri/02-ipc-contracts.md](tauri/02-ipc-contracts.md) | IPC command contracts between frontend and Rust commands. |
| [tauri/03-platform-targets.md](tauri/03-platform-targets.md) | Windows/macOS/Linux build targets, signing, notarization, installers. |
| [tauri/04-frontend-stack.md](tauri/04-frontend-stack.md) | React + Vite + Tailwind v4 + shadcn/ui structure, routing, state. |

### LLM adapters

| File | Description |
| --- | --- |
| [llm-adapters/00-providers-overview.md](llm-adapters/00-providers-overview.md) | Provider matrix, selection, CLI-first vs API fallback. |
| [llm-adapters/01-claude.md](llm-adapters/01-claude.md) | Claude CLI + Anthropic API adapter. |
| [llm-adapters/02-ollama.md](llm-adapters/02-ollama.md) | Ollama local CLI + HTTP adapter. |
| [llm-adapters/03-openai-codex.md](llm-adapters/03-openai-codex.md) | Codex CLI + OpenAI API adapter. |
| [llm-adapters/04-gemini.md](llm-adapters/04-gemini.md) | Gemini CLI + Google AI API adapter. |
| [llm-adapters/05-grok.md](llm-adapters/05-grok.md) | Grok CLI + xAI API adapter. |
| [llm-adapters/adapter-trait.md](llm-adapters/adapter-trait.md) | `LlmAdapter` trait definition and contract. |
| [llm-adapters/render-prompt.md](llm-adapters/render-prompt.md) | Canonical render prompt + system instructions. |

### Data sources

| File | Description |
| --- | --- |
| [data-sources/00-sources-overview.md](data-sources/00-sources-overview.md) | Source matrix, priority, `DataSource` trait. |
| [data-sources/01-local-git.md](data-sources/01-local-git.md) | Local git log gatherer (per-repo, windowed, author-filtered). |
| [data-sources/02-github.md](data-sources/02-github.md) | GitHub PRs + reviews via `gh` CLI. |
| [data-sources/03-claude-code.md](data-sources/03-claude-code.md) | Claude Code session transcript reader. |
| [data-sources/04-remember-plugin.md](data-sources/04-remember-plugin.md) | Remember plugin notes (today-*.md, .done.md, now.md). |
| [data-sources/05-opencode.md](data-sources/05-opencode.md) | OpenCode session + file readers. |
| [data-sources/06-codex.md](data-sources/06-codex.md) | Codex CLI session + file readers. |
| [data-sources/07-gemini-cli.md](data-sources/07-gemini-cli.md) | Gemini CLI session + file readers. |
| [data-sources/08-grok-cli.md](data-sources/08-grok-cli.md) | Grok CLI session + file readers. |

### Development

| File | Description |
| --- | --- |
| [dev/01-setup.md](dev/01-setup.md) | Toolchain (Rust, Node, pnpm, Tauri CLI), checkout, first build. |
| [dev/02-commands.md](dev/02-commands.md) | Make/pnpm/cargo command reference. |
| [dev/03-testing.md](dev/03-testing.md) | Unit, integration, e2e, snapshot strategy. |
| [dev/04-ci-cd.md](dev/04-ci-cd.md) | GitHub Actions: lint, test, build, release. |
| [dev/05-migration-from-appscript.md](dev/05-migration-from-appscript.md) | Mapping App Script behavior → autostand crates. |
| [dev/06-progress.md](dev/06-progress.md) | **Progress tracker** — phase-by-phase implementation status. Update at the end of every phase. |

### User guide

| File | Description |
| --- | --- |
| [user/01-install.md](user/01-install.md) | Install on Windows/macOS/Linux, scheduler setup. |
| [user/02-configuration.md](user/02-configuration.md) | Config file, provider selection, source toggles, host slug. |
| [user/03-daily-usage.md](user/03-daily-usage.md) | Day-to-day usage, manual edits, two-machine sync. |
| [user/04-troubleshooting.md](user/04-troubleshooting.md) | Common issues, logs, audit sidecars, self-heal. |

### Design system

The code these specs describe lives in [`MAECLY/autostand-ui`](https://github.com/MAECLY/autostand-ui) and is
consumed here as the `@autostand/ui` package. The specs stay in this repo because they are decisions, not code;
paths inside them are relative to the `autostand-ui` root unless stated otherwise.

| File | Description |
| --- | --- |
| [design-system/01-tokens.md](design-system/01-tokens.md) | Color, type, spacing, motion tokens. |
| [design-system/02-brand.md](design-system/02-brand.md) | Logo, mark, wordmark, usage. |
| [design-system/03-components.md](design-system/03-components.md) | Base shadcn/ui component inventory. |
| [design-system/04-app-components.md](design-system/04-app-components.md) | App-specific composite components — these stay in `apps/autostand-app/src/components/`. |
| [design-system/05-storybook.md](design-system/05-storybook.md) | Storybook 8 setup, stories, chromatic — runs from the `autostand-ui` repo. |
| [design-system/06-landing-reuse.md](design-system/06-landing-reuse.md) | How the design system is shared across the three repos, and what the split cost. |

### Specifications

| File | Description |
| --- | --- |
| [specs/standup-file-format.md](specs/standup-file-format.md) | AUTO/MANUAL block grammar, frontmatter, frozen marker. |
| [specs/configuration.md](specs/configuration.md) | Config schema, sources, providers, scheduler. |
| [specs/pipeline.md](specs/pipeline.md) | Canonical pipeline step list + invariants. |
| [specs/anti-backdating.md](specs/anti-backdating.md) | FORBIDDEN/COVERED/SKEW/CLAIM rules. |
| [specs/audit.md](specs/audit.md) | Audit sidecar JSON schema + phantom detector. |

---

## Quick links by audience

- **New developer** → `dev/01-setup.md` → `architecture/02-monorepo-structure.md` → `architecture/03-data-flow.md`.
- **End user** → `user/01-install.md` → `user/02-configuration.md` → `user/03-daily-usage.md`.
- **Designer** → `design-system/01-tokens.md` → `design-system/03-components.md` → `design-system/05-storybook.md`.
- **Contributor** → `architecture/01-overview.md` → `dev/03-testing.md` → `dev/04-ci-cd.md` → `dev/05-migration-from-appscript.md`.

---

## Legacy App Script reference

The original Github_Dailies App Script (Bash + Python) is the behavioral source of truth.
It is **not** part of this repo. Do not edit it. autostand reimplements its rules in
Rust; behavioral parity is enforced via the spec docs under `docs/specs/` and the
migration guide at `docs/dev/05-migration-from-appscript.md`.