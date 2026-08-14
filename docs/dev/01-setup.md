# Developer Setup

This guide walks you through getting the `autostand` development environment running from a clean clone.

## Prerequisites

| Tool | Version | Why |
|------|---------|-----|
| Rust | stable (via rustup) | Rust backend (Tauri + workspace crates) |
| Node.js | 20+ | Frontend tooling |
| pnpm | 9+ | Frontend package manager (monorepo) |
| git | any | Source control |
| `gh` CLI | latest, authenticated | GitHub data source (PRs, reviews) |
| At least one LLM CLI | — | Render provider. Any of: `claude`, `ollama`, `codex`, `gemini`, `grok` |

> **Note on LLM CLIs:** autostand auto-detects installed CLIs via `which`. For local development you only need one provider. Full coverage includes the five external provider CLIs plus the built-in local sidecar/runtime. Each is optional — `Auto` gracefully falls back to the deterministic renderer when no LLM is available.

### Installing prerequisites

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
rustup component add clippy rustfmt

# Node + pnpm (via fnm or nvm)
curl -fsSL https://fnm.vercel.app/install | bash
fnm install 20 && fnm use 20 && fnm default 20
corepack enable && corepack prepare pnpm@latest --activate

# gh CLI (macOS)
brew install gh
gh auth login
```

## Clone + first build

```bash
git clone https://github.com/MAECLY/autostand.git
cd autostand
pnpm install                    # frontend + design-system + storybook deps
cargo build --workspace         # Rust workspace
pnpm tauri dev                  # launch app in dev mode (hot-reload)
```

The first `cargo build` compiles Tauri + all workspace crates — expect 3–8 minutes depending on machine. Subsequent builds are incremental.

## Rust workspace setup

The repo root `Cargo.toml` declares the workspace:

```toml
[workspace]
resolver = "2"
members = [
    "crates/autostand-core",
    "crates/autostand-adapters",
    "crates/autostand-scheduler",
    "crates/autostand-tauri",
]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = "0.3"
thiserror = "1"
anyhow = "1"
regex = "1"
walkdir = "2"
```

Each member crate references shared deps with `serde.workspace = true`. Add new shared deps here, not in individual crates.

## Frontend setup

The app lives in `apps/autostand-app/` — a Vite + React + TypeScript project.

```bash
pnpm install
```

### Tailwind v4

Tailwind v4 is integrated via the Vite plugin (no `tailwind.config.js` by default — v4 uses CSS-first config):

```bash
pnpm add -D tailwindcss @tailwindcss/vite
```

`apps/autostand-app/vite.config.ts`:
```ts
import tailwindcss from "@tailwindcss/vite";
export default defineConfig({
  plugins: [react(), tailwindcss(), tailwindcssConfig()],
});
```

`apps/autostand-app/src/globals.css`:
```css
@import "tailwindcss";
@import "../../design-system/tokens/tokens.css";

@theme {
  --color-background: var(--bg-base);
  --color-foreground: var(--fg-base);
  --color-primary: var(--brand-primary);
  /* ... semantic mappings ... */
}
```

### shadcn/ui

```bash
pnpm dlx shadcn@latest init       # one-time init in apps/autostand-app/
pnpm dlx shadcn@latest add button card dialog input select switch tabs
```

Components are added under `apps/autostand-app/src/components/ui/`. The shared design-system copies live in `design-system/components/` — see `docs/design-system/03-components.md`.

## Storybook setup

Storybook lives in `design-system/`:

```bash
pnpm dlx storybook@latest init    # run inside design-system/
```

Config in `design-system/.storybook/`:
- `main.ts` — framework `@storybook/react-vite`, stories glob `../**/*.stories.tsx`
- `preview.ts` — imports `tokens.css` + sets light/dark backgrounds

See `docs/design-system/05-storybook.md` for full config.

## Design tokens setup

`design-system/tokens/tokens.css` contains all CSS variables (primitive, semantic, component). See `docs/design-system/01-tokens.md` for the full file.

Import in both:
- `apps/autostand-app/src/globals.css` (the Tauri app)
- `design-system/.storybook/preview.ts` (Storybook)

This guarantees the app and Storybook render identical colors, spacing, and typography.

## Env vars for dev

Set these in your shell (or `.envrc` / `.env`):

```bash
export GITHUB_DIR="$HOME/Documents/Github"     # where your work repos live
export DAILIES_DIR="$HOME/Sync/Github_Dailies"  # output dir (defaults to <repo>/dailies/)
```

For dev, `DAILIES_DIR` defaults to `<repo>/dailies/` if unset. Point it at your existing App Script dailies repo to test migration compatibility.

## Tauri dev

```bash
pnpm tauri dev
```

This:
1. Starts Vite at `localhost:1420`
2. Compiles the Rust backend (`crates/autostand-tauri`)
3. Launches the native window with hot-reload (frontend changes reload instantly; Rust changes trigger a rebuild + window restart)

If port 1420 is busy, Tauri will increment the port. Check `tauri.conf.json` for `devUrl`.

## CLAUDE.md / AGENTS.md

Create these in the repo root so AI agents (Claude Code, etc.) know the build/test commands:

```markdown
# AGENTS.md

## Build
- `cargo build --workspace` — Rust
- `pnpm build` — frontend
- `pnpm tauri build` — production bundle

## Test
- `cargo test --workspace` — Rust
- `pnpm test` — frontend (vitest)
- `pnpm test:e2e` — Playwright E2E

## Lint
- `cargo clippy --workspace -- -D warnings`
- `cargo fmt --all --check`
- `pnpm lint`
- `pnpm typecheck`

## Dev
- `pnpm tauri dev` — launches app
- `pnpm storybook` — design system
```

## Recommended IDE

**VS Code** with:
- `rust-analyzer` (Rust language server)
- `Tauri` extension (`tauri-apps.vscode-tauri`)
- `Tailwind CSS IntelliSense`
- `Even Better TOML`

or **Zed** (built-in Rust + TS support, fast).

## Verifying the setup

After `pnpm tauri dev`:
1. The app window opens showing the Dashboard (empty until you compile).
2. Check the status bar — it should show your host slug + "idle".
3. Go to Settings → Providers — you should see at least one CLI detected.
4. Go to Settings → Data Sources — toggle what you have.
5. Click "Compile now" — a standup file should appear in `DAILIES_DIR`.

If any of this fails, check `docs/user/04-troubleshooting.md`.
