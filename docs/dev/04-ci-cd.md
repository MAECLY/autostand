# CI/CD

autostand uses GitHub Actions for continuous integration and release. All workflows live in `.github/workflows/`.

## Workflows

### 1. `ci.yml` — Continuous integration

**Triggers:** `push` to `main`, `pull_request` to `main`.

**Jobs:**

```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace -- -D warnings
      - run: cargo test --workspace
      - run: cargo install cargo-audit && cargo audit

  frontend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm lint
      - run: pnpm typecheck
      - run: pnpm test
      - run: pnpm build
```

**CI runs (in order):**
1. `cargo fmt --all --check`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo test --workspace`
4. `cargo audit`
5. `pnpm lint`
6. `pnpm typecheck`
7. `pnpm test`
8. `pnpm build`

All must pass for a PR to merge (branch protection rule).

### 2. `release.yml` — Release builds

**Trigger:** tag `v*` (e.g., `v0.1.0`, `v1.0.0-beta.1`).

Uses `tauri-apps/tauri-action` to build per-platform bundles and upload to the GitHub Release.

```yaml
name: Release
on:
  push:
    tags: ['v*']

jobs:
  release:
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: macos-14
            args: '--target aarch64-apple-darwin'
          - platform: macos-13
            args: '--target x86_64-apple-darwin'
          - platform: ubuntu-22.04
            args: ''
          - platform: windows-latest
            args: ''
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.platform == 'macos-14' && 'aarch64-apple-darwin' || matrix.platform == 'macos-13' && 'x86_64-apple-darwin' || '' }}
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # macOS codesigning + notarization
          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
          APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
          # Windows codesigning
          WINDOWS_CERTIFICATE: ${{ secrets.WINDOWS_CERTIFICATE }}
          WINDOWS_CERTIFICATE_PASSWORD: ${{ secrets.WINDOWS_CERTIFICATE_PASSWORD }}
          # Tauri updater signing
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: 'autostand ${{ github.ref_name }}'
          releaseDraft: true
          prerelease: false
          args: ${{ matrix.args }}
```

### Build matrix

| OS | Target | Bundle format |
|----|--------|---------------|
| macOS 14 (Apple Silicon) | `aarch64-apple-darwin` | `.dmg`, `.app` |
| macOS 13 (Intel) | `x86_64-apple-darwin` | `.dmg`, `.app` |
| Ubuntu 22.04 | `x86_64-unknown-linux-gnu` | `.deb`, `.AppImage` |
| Windows latest | `x86_64-pc-windows-msvc` | `.msi`, `.exe` (NSIS) |

### 3. `storybook.yml` — Storybook deployment

**Trigger:** `push` to `main` (changes in `design-system/` only).

```yaml
name: Storybook
on:
  push:
    branches: [main]
    paths: ['design-system/**']

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm build-storybook
      - uses: peaceiris/actions-gh-pages@v3
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: design-system/storybook-static
```

Deployed to `https://MAECLY.github.io/autostand/storybook/`.

## Caching

| Cache | Action | What it caches |
|-------|--------|----------------|
| Cargo build cache | `Swatinem/rust-cache@v2` | `~/.cargo/registry`, `target/` — keyed on `Cargo.lock` |
| pnpm store | `actions/setup-node@v4` with `cache: pnpm` | `~/.local/share/pnpm/store` — keyed on `pnpm-lock.yaml` |

Caches reduce CI time from ~10min to ~3min on incremental changes.

## Secrets

Store these in GitHub repository settings → Secrets and variables → Actions.

| Secret | Used by | Purpose |
|--------|---------|---------|
| `APPLE_CERTIFICATE` | `release.yml` | macOS codesigning certificate (base64-encoded `.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | `release.yml` | Password for the certificate |
| `APPLE_ID` | `release.yml` | Apple ID for notarization |
| `APPLE_PASSWORD` | `release.yml` | App-specific password for notarization |
| `APPLE_TEAM_ID` | `release.yml` | Apple Developer Team ID |
| `WINDOWS_CERTIFICATE` | `release.yml` | Windows codesigning certificate (base64-encoded `.pfx`) |
| `WINDOWS_CERTIFICATE_PASSWORD` | `release.yml` | Password for the certificate |
| `TAURI_SIGNING_PRIVATE_KEY` | `release.yml` | Tauri updater signing key (for auto-updates) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | `release.yml` | Password for the updater key |

Until codesigning secrets are set, builds are unsigned (macOS users must right-click → Open to bypass Gatekeeper; Windows shows SmartScreen warning). See `docs/user/01-install.md`.

## Tauri updater

The app auto-updates on launch when a new release is available.

**`tauri.conf.json`** (excerpt):
```json
{
  "plugins": {
    "updater": {
      "active": true,
      "endpoints": [
        "https://github.com/MAECLY/autostand/releases/latest/download/latest.json"
      ],
      "pubkey": "<TAURI_SIGNING_PUBLIC_KEY>"
    }
  }
}
```

`release.yml` generates:
- The platform bundle (`.dmg`, `.deb`, `.msi`)
- A `latest.json` manifest (signed with `TAURI_SIGNING_PRIVATE_KEY`)
- A `.sig` signature file per bundle

On launch, the app fetches `latest.json`, compares versions, and prompts the user to update (or auto-updates, depending on settings).

## Release notes

Auto-generated from commits since the last tag. Format:

```markdown
## What's Changed

### Features
- <commit subject> (by @author)

### Fixes
- <commit subject>

### Other
- <commit subject>

**Full changelog:** https://github.com/MAECLY/autostand/compare/v0.1.0...v0.2.0
```

The `release.yml` job uses `release-drafter` or GitHub's auto-generated notes (configurable).

## Branch protection

The `main` branch has:
- Require pull request before merging
- Require status checks to pass (CI jobs)
- Require approvals: 1
- Require linear history
- Require branches up to date before merging

## Manual release process

```bash
# 1. Update version in:
#    - Cargo.toml (all crates)
#    - apps/autostand-app/package.json
#    - tauri.conf.json (version field)

# 2. Update CHANGELOG.md

# 3. Commit + tag
git commit -am "release: v0.2.0"
git tag v0.2.0
git push origin main --tags

# 4. release.yml triggers automatically
# 5. Draft release appears on GitHub — review + publish
```