# Build, Test, and Lint Commands

Comprehensive reference for all commands in the autostand monorepo. Run these from the repo root unless noted.

## Command reference

### Rust (workspace)

| Command | What it does |
|---------|--------------|
| `cargo build --workspace` | Build all Rust crates (autostand-core, autostand-adapters, autostand-scheduler, autostand-tauri) |
| `cargo build --release` | Release build (optimized) — used by `pnpm tauri build` |
| `cargo test --workspace` | Run all Rust unit + integration tests |
| `cargo test -p autostand-core` | Test only the core crate |
| `cargo test -p autostand-adapters` | Test only the adapters crate |
| `cargo test -p autostand-scheduler` | Test only the scheduler crate |
| `cargo test --test '*' -- --nocapture` | Run integration tests in `tests/` with stdout visible |
| `cargo clippy --workspace -- -D warnings` | Lint Rust (treat warnings as errors) |
| `cargo fmt --all --check` | Check formatting (use `cargo fmt --all` to fix) |
| `cargo audit` | Security audit dependencies (requires `cargo install cargo-audit`) |
| `cargo tarpaulin --workspace` | Coverage report (requires `cargo install cargo-tarpaulin`) |
| `cargo doc --workspace --no-deps --open` | Generate + open API docs |

### Frontend (app)

| Command | What it does |
|---------|--------------|
| `pnpm install` | Install frontend + design-system + storybook deps |
| `pnpm dev` | Start Vite dev server only (no Tauri window) — useful for UI-only work |
| `pnpm build` | Build frontend only (outputs `apps/autostand-app/dist/`) |
| `pnpm lint` | ESLint frontend |
| `pnpm typecheck` | `tsc --noEmit` — type-check without emit |
| `pnpm test` | Run frontend unit tests (vitest) |
| `pnpm test:watch` | Vitest in watch mode |
| `pnpm test:coverage` | Vitest with coverage report |
| `pnpm test:e2e` | Playwright E2E tests (launches the full Tauri app) |

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
| `pnpm dlx shadcn@latest add <comp>` | Add a shadcn component to the design system |

### Frontend deps (shadcn)

| Command | What it does |
|---------|--------------|
| `pnpm dlx shadcn@latest add button` | Add the Button component |
| `pnpm dlx shadcn@latest add card dialog sheet` | Add multiple components |
| `pnpm dlx shadcn@latest add --all` | Add all available shadcn components |

## Common workflows

### Before opening a PR

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
pnpm lint
pnpm typecheck
pnpm test
```

CI runs all of these — fixing them locally first saves a round-trip.

### Adding a new shadcn component

```bash
pnpm dlx shadcn@latest add accordion
# → component added to apps/autostand-app/src/components/ui/accordion.tsx
# → also copy to design-system/components/ if you want a Storybook story
```

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
2. `cargo clippy --workspace -- -D warnings`
3. `cargo test --workspace`
4. `cargo audit`
5. `pnpm lint`
6. `pnpm typecheck`
7. `pnpm test`
8. `pnpm build`

All must pass for a PR to merge. See `docs/dev/04-ci-cd.md` for full CI/CD config.

## Platform-specific notes

### macOS
- Requires Xcode Command Line Tools: `xcode-select --install`
- For Tauri builds targeting Apple Silicon + Intel: `rustup target add aarch64-apple-darwin x86_64-apple-darwin`

### Linux
- Tauri build deps (Debian/Ubuntu): `sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`
- For AppImage: AppImageKit (bundled by tauri-action)

### Windows
- Requires MSVC toolchain (installed via Visual Studio Build Tools)
- WebView2 runtime (preinstalled on Windows 10/11)

## Troubleshooting commands

| Symptom | Fix |
|---------|-----|
| `pnpm tauri dev` hangs on first run | Wait for initial Rust compile. Check `cargo build --workspace` succeeds standalone. |
| `cargo test` fails with "linker not found" | Install platform build tools (see above). |
| `pnpm storybook` blank page | Check `design-system/.storybook/preview.ts` imports `tokens.css`. |
| `cargo audit` not found | `cargo install cargo-audit` |