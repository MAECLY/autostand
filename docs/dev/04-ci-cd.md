# CI/CD

autostand uses GitHub Actions for continuous integration and release. All workflows live in `.github/workflows/`.

## Workflows

### 1. `ci.yml` — Continuous integration

**Triggers:** `push` to `main`, `pull_request` to `main`.

`permissions: contents: read` — CI reads the repo and publishes nothing. `concurrency` is keyed on
`github.ref` with `cancel-in-progress` only for pull requests, so pushing a fixup supersedes the
in-flight PR run while a push to `main` always runs to completion.

Two jobs, both `ubuntu-latest`, both with a `timeout-minutes` and no `continue-on-error` anywhere —
a step that fails must fail the job.

`.github/workflows/ci.yml` is the source of truth. Abridged (the real file carries comments
explaining every non-obvious line):

```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

permissions:
  contents: read

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

jobs:
  rust:
    runs-on: ubuntu-latest
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@v4
      - name: Install Tauri v2 Linux system dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y --no-install-recommends \
            build-essential curl wget file pkg-config \
            libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
            libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
      - run: cargo install cargo-audit --locked --version '^0.22'
      - run: cargo audit

  frontend:
    runs-on: ubuntu-latest
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4      # version comes from packageManager
      - uses: actions/setup-node@v4
        with:
          node-version: "22"
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm lint
      - run: pnpm typecheck
      - run: pnpm test
      - run: pnpm build:web
```

**CI runs (in order):**

| # | Job | Command |
|---|-----|---------|
| 1 | rust | `cargo fmt --all --check` |
| 2 | rust | `cargo clippy --workspace --all-targets -- -D warnings` |
| 3 | rust | `cargo test --workspace` |
| 4 | rust | `cargo audit` |
| 5 | frontend | `pnpm install --frozen-lockfile` |
| 6 | frontend | `pnpm lint` |
| 7 | frontend | `pnpm typecheck` |
| 8 | frontend | `pnpm test` |
| 9 | frontend | `pnpm build:web` |

All must pass for a PR to merge (branch protection rule).

#### Why the workflow is not literally what this doc first described

This section was written in F0, before the code existed. Five things in it were wrong once the
repo caught up, and the workflow deliberately diverges:

- **`--all-targets` on clippy.** Without it, clippy lints only the default targets and skips every
  `#[cfg(test)]` module and every file under `tests/`. The workspace `[lints]` table (rust
  `unsafe_code = deny`, clippy `all` + `pedantic` + `cargo`) is the standard the whole repo is held
  to, test code included, so CI lints all targets.
- **Linux system libraries.** Every cargo command compiles `apps/autostand-app/src-tauri`, which
  links GTK3/WebKitGTK. The package list is derived from this repo's `Cargo.lock`, not from the
  generic Tauri prerequisites page: `webkit2gtk-sys 2.0.2` probes pkg-config `webkit2gtk-4.1`
  (**4.1**, not 4.0), `soup3-sys` probes `libsoup-3.0`, `gtk-sys 0.18` probes `gtk+-3.0`, and
  `libdbus-sys 0.2.7` probes `dbus-1` — pulled in by `tao 0.35`'s default `dbus` feature, which is
  why **`libdbus-1-dev`** is on the list even though the usual copy-pasted snippet omits it.
  `libssl-dev` is *not* installed: `reqwest` is `default-features = false` with `rustls-tls`, so
  `openssl-sys` is absent from the graph. `libxdo-dev` is *not* installed either: there is no
  `libxdo-sys` in `Cargo.lock`.
- **`pnpm build` cannot run in the frontend job.** The root `build` script is `tauri build`; it needs
  the Rust toolchain and the same GTK/WebKit stack, and it emits a desktop bundle. That belongs in
  `release.yml`. CI calls `pnpm build:web` instead, a root script that builds the three web surfaces
  in order — `build:frontend` (Vite, `apps/autostand-app`), `build:landing` (Astro, `apps/landing`),
  `build-storybook` (`design-system` → `design-system/storybook-static`). `build:frontend`, `lint`,
  `typecheck` and `test` keep their existing meanings.
- **pnpm version.** The repo pins `pnpm@11.18.0` via `packageManager`, not 9. `pnpm/action-setup@v4`
  is given no `version:` input so it reads that field; hardcoding a version here would just be a
  second place to forget to update.
- **Node version.** Root `engines` says `node >= 20`, but `@testing-library/jest-dom@6.10.0` (a dev
  dependency of `apps/autostand-app`) declares `engines.node >= 22`. CI runs Node 22, the real floor.

#### cargo audit

`cargo audit` exits non-zero for *vulnerabilities*. `unmaintained` and `unsound` advisories are
reported as warnings and do not fail the build, which is the behaviour CI relies on. The workspace
currently has **0 vulnerabilities and 17 warnings** across 541 crates, and **none of them are
suppressed** — there is no `--ignore` flag in the workflow:

| Advisories | Crates | Kind |
|------------|--------|------|
| RUSTSEC-2024-0411 … -0420 (10) | `atk`, `atk-sys`, `gdk`, `gdk-sys`, `gdkwayland-sys`, `gdkx11`, `gdkx11-sys`, `gtk`, `gtk-sys`, `gtk3-macros` | unmaintained — gtk-rs GTK3 bindings |
| RUSTSEC-2024-0429 | `glib 0.18.5` | unsound — `VariantStrIter` iterator impls |
| RUSTSEC-2024-0370 | `proc-macro-error` | unmaintained |
| RUSTSEC-2025-0075/-0080/-0081/-0098/-0100 | `unic-common`, `unic-ucd-ident`, `unic-char-property`, `unic-ucd-version`, `unic-char-range` | unmaintained |

Every one arrives transitively through Tauri's Linux GTK3 stack or through build-time proc macros;
none is reachable from autostand's own code and none has a fixed version to upgrade to. They clear
when Tauri's Linux backend moves off GTK3. If a real vulnerability ever lands, the step fails — that
is the point of running it unsuppressed.

### 2. `release.yml` — Release builds

**Trigger:** tag `v*` (e.g., `v0.1.0`, `v1.0.0-beta.1`), plus a `workflow_dispatch` that takes an
existing tag as input — the re-run path when one platform of a release fails.

`permissions: contents: write` is declared at the workflow level rather than inherited from the
default token scope: the workflow creates a draft release and uploads assets, and needs nothing else.
`concurrency` is keyed on the tag with `cancel-in-progress: false` — a half-uploaded draft release is
worse than a slow one.

Two jobs:

1. **`version-check`** (ubuntu-latest) — runs `tests/verify-version-consistency.py "$RELEASE_TAG"`,
   which fails when the workspace `Cargo.toml`, the four `package.json` files and `tauri.conf.json`
   do not all carry the tagged version. A cheap gate in front of four expensive builds; without it a
   forgotten bump ships a binary and a bundle stamped with different versions.
2. **`release`** — the four-platform matrix below, `fail-fast: false`, each row calling
   `tauri-apps/tauri-action@v0` with `releaseDraft: true`. Nothing publishes automatically: a human
   reviews the draft, smoke-tests a bundle, and presses publish.

`.github/workflows/release.yml` is the source of truth. Abridged:

```yaml
name: Release
on:
  push:
    tags: ['v*']
  workflow_dispatch:
    inputs:
      tag: { description: 'Existing tag to build (e.g. v0.2.0)', required: true, type: string }

permissions:
  contents: write

env:
  RELEASE_TAG: ${{ github.event.inputs.tag || github.ref_name }}

jobs:
  version-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { ref: '${{ env.RELEASE_TAG }}' }
      - run: python3 tests/verify-version-consistency.py "$RELEASE_TAG"

  release:
    needs: version-check
    strategy:
      fail-fast: false
      matrix:
        include:
          - { platform: macos-14,       target: aarch64-apple-darwin,   args: '--target aarch64-apple-darwin' }
          - { platform: macos-13,       target: x86_64-apple-darwin,    args: '--target x86_64-apple-darwin' }
          - { platform: ubuntu-22.04,   target: x86_64-unknown-linux-gnu, args: '' }
          - { platform: windows-latest, target: x86_64-pc-windows-msvc, args: '' }
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4
        with: { ref: '${{ env.RELEASE_TAG }}' }
      - if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y --no-install-recommends \
            libwebkit2gtk-4.1-dev libsoup-3.0-dev libgtk-3-dev libayatana-appindicator3-dev \
            librsvg2-dev libdbus-1-dev libxdo-dev libssl-dev build-essential pkg-config \
            file wget curl patchelf
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: '${{ matrix.target }}' }
      - uses: Swatinem/rust-cache@v2
        with: { cache-targets: 'false', key: '${{ matrix.target }}' }
      - uses: pnpm/action-setup@v4          # version comes from `packageManager`
      - uses: actions/setup-node@v4
        with: { node-version: 20, cache: pnpm }
      - run: pnpm install --frozen-lockfile
      - run: pnpm --filter autostand-app build     # tauri.conf.json has no beforeBuildCommand
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # macOS codesigning + notarization (see § Secrets)
          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          KEYCHAIN_PASSWORD: ${{ secrets.KEYCHAIN_PASSWORD }}
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
          APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
          # Windows codesigning — reserved, see below
          WINDOWS_CERTIFICATE: ${{ secrets.WINDOWS_CERTIFICATE }}
          WINDOWS_CERTIFICATE_PASSWORD: ${{ secrets.WINDOWS_CERTIFICATE_PASSWORD }}
        with:
          projectPath: apps/autostand-app
          tauriScript: pnpm tauri
          tagName: ${{ env.RELEASE_TAG }}
          releaseName: autostand ${{ env.RELEASE_TAG }}
          releaseDraft: true
          prerelease: false
          args: ${{ matrix.args }}
```

**Why it differs from a stock `tauri-action` example** — every item here is load-bearing:

- **The frontend build is an explicit step.** `apps/autostand-app/src-tauri/tauri.conf.json` defines
  no `beforeBuildCommand`, so `tauri build` does not build the web assets — it only copies
  `frontendDist` (`../dist`). Drop `pnpm --filter autostand-app build` and the bundle ships stale or
  missing UI, without failing.
- **`projectPath: apps/autostand-app` + `tauriScript: pnpm tauri`.** `src-tauri/` is not at the repo
  root, and the pnpm lockfile is, so package-manager auto-detection inside `apps/autostand-app` can
  guess wrong. `pnpm tauri` resolves the app's own `@tauri-apps/cli` (tauri-cli 2.x).
- **pnpm is not pinned in the workflow.** `pnpm/action-setup@v4` with no `version` reads
  `packageManager` (`pnpm@11.18.0`) from the root `package.json`, so the workflow cannot drift from
  the repo. Node is `20`, matching `engines.node >= 20`.
- **Linux system libraries are installed explicitly** — the Tauri v2 Debian/Ubuntu prerequisites, a
  superset of what the Linux job in `ci.yml` installs (that job only has to compile; this one also
  bundles) plus `patchelf`, which the AppImage bundler shells out to. Nothing links against webkit
  without them.
- **`Swatinem/rust-cache@v2` runs with `cache-targets: false`.** A release `target/` is multi-GB with
  `lto = true` / `codegen-units = 1`; caching it would evict the CI caches every PR depends on, out
  of the 10 GB repo budget. Only the registry/index is cached; a release compiles from scratch.
- **Rust targets come from `matrix.target`** instead of a nested `&&`/`||` ternary over the runner
  name. Passing the host triple on Linux/Windows is a no-op for `rustup`.

**Signing secrets** are referenced by name only; all are empty until they are configured, and an
empty `APPLE_CERTIFICATE` makes `tauri-action` produce an unsigned build rather than fail.

- `APPLE_*` and `KEYCHAIN_PASSWORD` are consumed by `tauri-action` (keychain import, then
  notarization via `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID`). `APPLE_SIGNING_IDENTITY` and
  `KEYCHAIN_PASSWORD` are referenced by the workflow but not yet listed in § Secrets — add them there
  when macOS signing is set up.
- `WINDOWS_CERTIFICATE` / `WINDOWS_CERTIFICATE_PASSWORD` are **reserved, not active**: Tauri v2 signs
  Windows bundles from `bundle > windows > certificateThumbprint` (or a `signCommand`) in
  `tauri.conf.json`, and neither is configured. Passing the env vars alone signs nothing.
- `TAURI_SIGNING_PRIVATE_KEY` / `..._PASSWORD` are **not wired**, because the updater is not enabled.
  See § Tauri updater.

### Build matrix

| OS | Target | Bundle format |
|----|--------|---------------|
| macOS 14 (Apple Silicon) | `aarch64-apple-darwin` | `.dmg`, `.app` |
| macOS 13 (Intel) | `x86_64-apple-darwin` | `.dmg`, `.app` |
| Ubuntu 22.04 | `x86_64-unknown-linux-gnu` | `.deb`, `.rpm`, `.AppImage` |
| Windows latest | `x86_64-pc-windows-msvc` | `.msi` (WiX), `.exe` (NSIS) |

The bundle list per platform follows from `"bundle": { "targets": "all" }` in `tauri.conf.json`;
Ubuntu 22.04 is the oldest glibc supported and must not be bumped casually — bundles built on a newer
runner will not start on 22.04.

### 3. `pages.yml` — Web surfaces (landing page + Storybook)

**Triggers:** `push` to `main`, `workflow_dispatch`.

A repository gets exactly **one** GitHub Pages site, and this repo has two web
surfaces to publish. So there is one workflow, not two: it builds both, assembles
them into a single directory, and uploads that once.

| URL | Source | Build output |
|-----|--------|--------------|
| `https://MAECLY.github.io/autostand/` | `apps/landing` (Astro 5) | `apps/landing/dist` |
| `https://MAECLY.github.io/autostand/storybook/` | `design-system` (Storybook 8) | `design-system/storybook-static` |

It uses the first-party Pages flow (`configure-pages` → `upload-pages-artifact` →
`deploy-pages`), not a third-party push-to-`gh-pages`-branch action. That flow
needs `pages: write` + `id-token: write` (OIDC) + `contents: read`, and a
`concurrency` group so two pushes cannot race a deployment.

```yaml
name: Pages
on:
  push:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: false   # never cancel a deploy mid-swap

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4        # version comes from `packageManager`
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: pnpm
      - uses: actions/configure-pages@v5
      - run: pnpm install --frozen-lockfile
      - run: pnpm --filter landing build
      - run: pnpm --filter design-system build-storybook
      - name: Assemble site
        run: |
          rm -rf _site && mkdir -p _site/storybook
          cp -R apps/landing/dist/. _site/
          cp -R design-system/storybook-static/. _site/storybook/
          touch _site/.nojekyll
      - name: Verify base paths        # see below — this is the real failure mode
        run: ...
      - uses: actions/upload-pages-artifact@v3
        with:
          path: _site

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

#### Base paths

The site is served from `/autostand/`, not from a domain root, so every asset URL
in both surfaces has to account for that prefix. A wrong base path deploys green
and then 404s every stylesheet and script — which is why the workflow asserts it
rather than trusting it.

- **Landing page** — `apps/landing/astro.config.mjs` sets `base: "/autostand"`,
  and Astro bakes it into the emitted markup: `href="/autostand/_astro/…"`,
  `src="/autostand/brand/…"`. Root-absolute, so the build drops straight in at
  the artifact root.
- **Storybook** — needs no base configuration. `@storybook/builder-vite` builds
  with Vite's `base` set to `"./"`, so `index.html`/`iframe.html` reference
  `./assets/…`, and `__vitePreload` resolves each chunk's dependency list
  against that chunk's own `import.meta.url`. The bundle is position-independent
  and works at any depth. (Verified: the same `storybook-static` bytes serve
  cleanly from both `/autostand/storybook/` and `/a/b/c/storybook/`.) If that ever
  stops being true, the fix is `config.base = "/autostand/storybook/"` inside the
  existing `viteFinal` hook in `design-system/.storybook/main.ts` — `storybook
  build` has no `--base` flag.

The `Verify base paths` step therefore fails the build when:

1. `_site/index.html` has a root-absolute `href`/`src` that is **not** under
   `/autostand/` (someone changed or dropped the Astro `base`);
2. a URL the landing page references is not actually in the artifact;
3. `_site/storybook/index.html` or `iframe.html` contains **any** root-absolute
   `href`/`src` (Storybook stopped emitting a relative bundle).

`.nojekyll` is defensive only. The artifact flow serves the upload verbatim and
never runs Jekyll; the file exists so `_astro/` survives if Pages is ever
switched back to branch-based publishing, where Jekyll strips `_`-prefixed
directories.

#### Why no `paths:` filter

The old design triggered only on `design-system/**`. With one combined artifact
that would publish a site missing whichever surface did not change, so the
workflow rebuilds both on every push to `main`. Both builds are a few seconds.

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

**The updater is not enabled.** The app does not check for updates, and `release.yml` produces no
`latest.json` and no `.sig` files. Users update by downloading the newer bundle from the release page.

Earlier revisions of this document showed a `plugins.updater` block with a `pubkey` as if it were
configured. It never was: `apps/autostand-app/src-tauri/tauri.conf.json` has no `updater` entry, and
`apps/autostand-app/src-tauri/Cargo.toml` has no updater plugin dependency. The
`TAURI_SIGNING_PRIVATE_KEY*` rows in § Secrets are therefore reserved, not used.

### What enabling it requires

Five changes, in this order. Steps 2–4 must land together — a `plugins.updater` block without the
registered plugin is inert, and an `updater:*` permission without the crate fails capability
resolution at build time.

1. **Generate a keypair** — `pnpm -C apps/autostand-app tauri signer generate -w ~/.tauri/autostand.key`.
   The private key and its password become the `TAURI_SIGNING_PRIVATE_KEY` /
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets. The **public** key gets committed —
   `tauri build` reads it out of `tauri.conf.json`, so it cannot be injected from a GitHub secret at
   build time, and it does not need to be: a public key is not a credential.
2. **Crate dependency + registration** — `tauri-plugin-updater = "2"` in
   `apps/autostand-app/src-tauri/Cargo.toml`, and
   `.plugin(tauri_plugin_updater::Builder::new().build())` on the builder in
   `apps/autostand-app/src-tauri/src/lib.rs`.
3. **Config** — in `tauri.conf.json`:
   ```json
   {
     "bundle": { "createUpdaterArtifacts": true },
     "plugins": {
       "updater": {
         "endpoints": [
           "https://github.com/MAECLY/autostand/releases/latest/download/latest.json"
         ],
         "pubkey": "<paste the generated .pub contents here>"
       }
     }
   }
   ```
   `createUpdaterArtifacts` is what makes the bundler emit the `.sig` files and `latest.json`;
   without it the signing key is ignored.
4. **Capability** — add `updater:default` to `permissions` in
   `apps/autostand-app/src-tauri/capabilities/default.json`.
5. **Frontend** — add `@tauri-apps/plugin-updater` and call `check()` from somewhere (app start, or a
   "check for updates" action). Nothing updates on its own; the plugin only exposes the API.

Then restore the two env vars in `release.yml`'s `tauri-action` step:

```yaml
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
```

and `tauri-action` will attach a signed `latest.json` plus a `.sig` per bundle to the draft release.
Note the update flow only works from a **published** release — `endpoints` points at
`releases/latest`, which skips drafts.

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

Not automated today: `release.yml` passes a fixed `releaseBody` (what the bundles are, and the
unsigned-build caveat) and points at `CHANGELOG.md`. The draft release is editable before publishing,
and GitHub's "Generate release notes" button produces the layout above on demand. Wiring
`release-drafter` is optional and not configured.

## Branch protection

The `main` branch has:
- Require pull request before merging
- Require status checks to pass (CI jobs)
- Require approvals: 1
- Require linear history
- Require branches up to date before merging

## Manual release process

The version lives in six files and they must all agree — cargo stamps the binary from the workspace
`Cargo.toml`, `tauri build` stamps the bundle from `tauri.conf.json`, and nothing reconciles the two.

```bash
# 1. Bump the version in every manifest:
#      Cargo.toml                                    -> [workspace.package] version
#      package.json                                  -> "version"   (repo root)
#      apps/autostand-app/package.json               -> "version"
#      apps/landing/package.json                     -> "version"
#      design-system/package.json                    -> "version"
#      apps/autostand-app/src-tauri/tauri.conf.json  -> "version"
#    The member crates inherit `version.workspace = true` — leave those alone.

# 2. Prove it. Fails loudly, naming every file that disagrees.
python3 tests/verify-version-consistency.py v0.2.0

# 3. CHANGELOG.md: move the [Unreleased] entries under a dated `## [0.2.0] - YYYY-MM-DD`
#    heading and update the link references at the bottom.

# 4. Commit + tag
git commit -am "release: v0.2.0"
git tag v0.2.0
git push origin main --tags

# 5. release.yml triggers on the tag: version-check runs the same script above,
#    then the four platform builds run in parallel.
# 6. A DRAFT release appears on GitHub. Download one bundle, smoke-test it, then publish.
```

Step 2 is also step 1 of the workflow, so a forgotten file fails the release in ~20 seconds instead
of after four full platform builds. If a build fails for one platform only, fix it and re-run via
**Actions → Release → Run workflow** with the same tag — the other bundles already attached to the
draft are kept.