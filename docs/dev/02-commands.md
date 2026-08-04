# Build, Test, and Lint Commands

Comprehensive reference for all commands in the autostand monorepo. Run these from the repo root unless noted.

## Makefile

`make` is the front door — run it with no arguments to list every target. It is a thin layer over cargo, pnpm and
tauri, so everything below can still be invoked directly.

| Target | What it does |
|--------|--------------|
| `make dev` | Run the desktop app with hot reload (Vite + Rust) |
| `make dev-web` | Vite dev server only, no Tauri window — for UI-only work |
| `make dev-landing` | The marketing landing page dev server |
| `make storybook` | Storybook on `:6006` |
| `make setup` | First-time setup: JS deps, Rust build, Playwright browser |
| `make build` | Desktop bundles for this platform |
| `make build-web` | The three web surfaces (app, landing, Storybook) |
| `make test` | Rust + frontend unit suites |
| `make test-e2e` | Both Playwright suites |
| `make lint` / `make fmt` / `make typecheck` / `make audit` | Quality gates |
| `make check` | **Everything CI runs** — do this before pushing |
| `make compile` | Compile a standup headlessly, the way the scheduler does (`DATE=YYYY-MM-DD` optional) |
| `make brand` | Regenerate the logo suite, app icons and the OG card |
| `make versions` | Check the version is consistent across every manifest |
| `make clean` / `make clean-all` | Remove build output / also deps and `target/` |

`make compile` runs the **real** pipeline: it reads your repos, writes `<date>.md` into your dailies directory
and commits + pushes it. It is the product doing its job, not a dry run.

Dependency installs are automatic — targets that need `node_modules` depend on it, so a stale checkout installs
before it runs.

## Command reference

### Rust (workspace)

| Command | What it does |
|---------|--------------|
| `cargo build --workspace` | Build all Rust crates (autostand-core, autostand-adapters, autostand-scheduler, autostand-app) |
| `cargo build --release` | Release build (optimized) — used by `pnpm tauri build` |
| `cargo test --workspace` | Run all Rust unit + integration tests |
| `cargo test -p autostand-core` | Test only the core crate |
| `cargo test -p autostand-adapters` | Test only the adapters crate |
| `cargo test -p autostand-scheduler` | Test only the scheduler crate |
| `cargo test --test '*' -- --nocapture` | Run integration tests in `tests/` with stdout visible |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint Rust including test targets (warnings are errors) |
| `cargo fmt --all --check` | Check formatting (use `cargo fmt --all` to fix) |
| `cargo audit` | Security audit dependencies (requires `cargo install cargo-audit`) |
| `cargo tarpaulin --workspace` | Coverage report (requires `cargo install cargo-tarpaulin`) |
| `cargo doc --workspace --no-deps --open` | Generate + open API docs |

### Frontend (app)

| Command | What it does |
|---------|--------------|
| `pnpm install` | Install frontend + design-system + storybook deps |
| `pnpm dev` | Alias for `pnpm tauri dev` — the full app. For Vite alone use `pnpm --filter autostand-app dev` |
| `pnpm build` | Alias for `pnpm tauri build` — desktop bundles, not a frontend build |
| `pnpm build:frontend` | Build the app frontend only (outputs `apps/autostand-app/dist/`) |
| `pnpm build:landing` | Build the landing page (outputs `apps/landing/dist/`) |
| `pnpm build:web` | Build all three web surfaces (app, landing, Storybook) |
| `pnpm lint` | ESLint across the three JS packages |
| `pnpm typecheck` | Type-check the three JS packages (`tsc --noEmit`, `astro check` for the landing) |
| `pnpm test` | Frontend unit tests (vitest) |
| `pnpm test:e2e` | Playwright E2E for the app; the landing suite is `pnpm --filter landing test:e2e` |

### Tauri (app bundling)

| Command | What it does |
|---------|--------------|
| `pnpm tauri dev` | Dev mode: Vite + Rust backend with hot-reload |
| `pnpm tauri build` | Production bundles (per-platform: `.dmg`/`.app`, `.deb`/`.AppImage`, `.msi`/`.exe`) |
| `pnpm tauri build --debug` | Production build with debug symbols (for profiling) |
| `pnpm tauri info` | Print Tauri environment diagnostics |

### Design system + Storybook

| Command | What it does |
|---------|--------------|
| `pnpm storybook` | Run Storybook dev server at `localhost:6006` (in `design-system/`) |
| `pnpm build-storybook` | Build static Storybook to `design-system/storybook-static/` |

Base components are hand-written into `design-system/components/` rather than added with the shadcn CLI — see
`docs/tauri/04-frontend-stack.md` § shadcn/ui components for why.

## Common workflows

### Before opening a PR

```bash
make check
```

That is exactly what CI runs, in the same order. Fixing it locally first saves a round-trip.

### Adding a base component

Write it into `design-system/components/<name>.tsx` alongside a `<name>.stories.tsx`, importing `cn` from
`../lib/utils` (the `@/` alias means something different in each package, so it is banned there). Export it from
`design-system/components/index.ts`. Apps consume it as `@design-system/components/<name>`.

### Adding a new Rust crate

1. `mkdir crates/<new-crate>`
2. Create `crates/<new-crate>/Cargo.toml` with `[package]` + `[dependencies]`
3. Add `crates/<new-crate>` to `[workspace] members` in root `Cargo.toml`
4. `cargo build --workspace` to verify

### Running a single test

```bash
cargo test -p autostand-core --lib dates::tests::next_business_day    # single unit test
cargo test -p autostand-core --test pipeline                          # single integration test file
pnpm test -- src/components/AuditViewer                               # single vitest file
```

## CI commands

The CI pipeline (`.github/workflows/ci.yml`) runs:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cargo audit`
5. `pnpm lint`
6. `pnpm typecheck`
7. `pnpm test`
8. `pnpm build:web`
9. Both Playwright suites

`make check` runs all of them locally. All must pass for a PR to merge. See `docs/dev/04-ci-cd.md`.

## Platform-specific notes

### macOS
- Requires Xcode Command Line Tools: `xcode-select --install`
- For Tauri builds targeting Apple Silicon + Intel: `rustup target add aarch64-apple-darwin x86_64-apple-darwin`

### Linux
- Tauri build deps (Debian/Ubuntu) — the same set `.github/workflows/ci.yml` installs:
  `sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev`
- For AppImage: AppImageKit (bundled by tauri-action)

### Windows
- Requires MSVC toolchain (installed via Visual Studio Build Tools)
- WebView2 runtime (preinstalled on Windows 10/11)

## Troubleshooting commands

| Symptom | Fix |
|---------|-----|
| `pnpm tauri dev` hangs on first run | Wait for initial Rust compile. Check `cargo build --workspace` succeeds standalone. |
| `cargo test` fails with "linker not found" | Install platform build tools (see above). |
| `pnpm storybook` blank page | Check `design-system/.storybook/preview.ts` imports `../styles/globals.css`. |
| `cargo audit` not found | `make audit` installs it on first use |
| `make` target not found | You need GNU Make (macOS ships 3.81, which is enough) |