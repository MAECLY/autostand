# Tauri v2 Project Setup

This document covers bootstrapping the `autostand` Tauri v2 desktop app: prerequisites, workspace layout, Cargo manifests, `tauri.conf.json`, the main entrypoints, capability files, frontend Vite config, dev commands, and platform-specific notes.

---

## Prerequisites

| Tool | Min version | Notes |
| --- | --- | --- |
| Rust (stable) | 1.78+ | `rustup default stable` |
| Node.js | 20 LTS+ | use `fnm`/`nvm` if managing multiple versions |
| pnpm | 9+ | `corepack enable && corepack prepare pnpm@latest --activate` |
| Git | 2.30+ | required for dailies repo sync + repo discovery |

### System dependencies per platform

**macOS**

```bash
xcode-select --install
```

Requires Xcode Command Line Tools for the C/C++ toolchain that `rustc` links against.

**Linux (Debian/Ubuntu)**

```bash
sudo apt update && sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  pkg-config
```

**Windows**

- WebView2 Runtime (preinstalled on Windows 10/11; if missing, install from <https://developer.microsoft.com/microsoft-edge/webview2/>).
- MSVC build tools (`rustup` will auto-select the `x86_64-pc-windows-msvc` target).
- For NSIS or WiX bundling, Tauri auto-detects installed toolchain.

---

## Init

### Scaffolded path

```bash
pnpm create tauri-app
# Project name: autostand
# Identifier: com.miguel50flowers.autostand
# Frontend: React + TypeScript
# Bundler: Vite
# Package manager: pnpm
```

This produces `apps/autostand-app/` with `src/` (React) and `src-tauri/` (Rust). The repo then becomes a Cargo workspace by adding a root `Cargo.toml` and relocating Rust crates under `crates/`.

### Manual structure (target layout)

```
autostand/
├── Cargo.toml                          # workspace root
├── crates/
│   ├── autostand-core/                 # domain model, pipeline, format
│   │   └── Cargo.toml
│   ├── autostand-adapters/             # git/github/jira/llm adapters
│   │   └── Cargo.toml
│   └── autostand-scheduler/            # cron + system-scheduler install
│       └── Cargo.toml
├── apps/
│   └── autostand-app/
│       ├── package.json
│       ├── vite.config.ts
│       ├── tsconfig.json
│       ├── index.html
│       ├── src/                        # React frontend
│       └── src-tauri/
│           ├── Cargo.toml
│           ├── tauri.conf.json
│           ├── build.rs
│           ├── capabilities/
│           │   └── default.json
│           └── src/
│               ├── main.rs
│               └── lib.rs
├── design-system/
│   └── tokens/
│       └── tokens.css
└── docs/
```

---

## Workspace `Cargo.toml`

The root manifest declares all Rust members and hoists shared dependencies so versions stay in sync.

```toml
# /Cargo.toml
[workspace]
resolver = "2"
members = [
    "crates/autostand-core",
    "crates/autostand-adapters",
    "crates/autostand-scheduler",
    "apps/autostand-app/src-tauri",
]

[workspace.dependencies]
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
tokio        = { version = "1", features = ["full"] }
anyhow       = "1"
thiserror    = "1"
tracing      = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
chrono       = { version = "0.4", features = ["serde"] }
regex        = "1"
rusqlite      = { version = "0.31", features = ["bundled"] }   # for run log / cache index
keyring      = "2"
dirs         = "5"

[profile.release]
codegen-units = 1
lto           = "thin"
strip         = "symbols"
panic         = "abort"
```

Member crates pull from the workspace with `dep = { workspace = true }`:

```toml
# crates/autostand-core/Cargo.toml
[package]
name    = "autostand-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde       = { workspace = true }
serde_json  = { workspace = true }
anyhow      = { workspace = true }
thiserror   = { workspace = true }
tracing     = { workspace = true }
chrono      = { workspace = true }
regex       = { workspace = true }
```

---

## `apps/autostand-app/src-tauri/Cargo.toml`

The Tauri shell wires the three library crates together and registers all plugins.

```toml
[package]
name    = "autostand-app"
version = "0.1.0"
edition = "2021"

[lib]
name = "autostand_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
autostand-core      = { path = "../../../crates/autostand-core" }
autostand-adapters  = { path = "../../../crates/autostand-adapters" }
autostand-scheduler = { path = "../../../crates/autostand-scheduler" }

tauri                       = { version = "2", features = ["tray-icon"] }
tauri-plugin-stronghold     = "2"          # encrypted local vault (optional alt to keyring)
tauri-plugin-fs             = "2"
tauri-plugin-shell          = "2"
tauri-plugin-store          = "2"
tauri-plugin-dialog         = "2"
tauri-plugin-notification   = "2"
tauri-plugin-opener         = "2"          # hand a path/URL to the OS shell (`open_in_file_manager`)

serde       = { workspace = true }
serde_json  = { workspace = true }
tokio       = { workspace = true }
anyhow      = { workspace = true }
thiserror   = { workspace = true }
tracing     = { workspace = true }
chrono      = { workspace = true }
dirs        = { workspace = true }
keyring     = { workspace = true }

[target.'cfg(target_os = "macos")'.dependencies]
tauri-plugin-stronghold = "2"

[features]
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
```

---

## `tauri.conf.json`

```jsonc
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "autostand",
  "version": "0.1.0",
  "identifier": "com.miguel50flowers.autostand",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build"
  },
  "app": {
    "windows": [
      {
        "title": "autostand",
        "width": 1280,
        "height": 800,
        "minWidth": 960,
        "minHeight": 600,
        "resizable": true,
        "fullscreen": false
      }
    ],
    "security": {
      "csp": "default-src 'self'; img-src 'self' data: https://avatars.githubusercontent.com https://*.githubusercontent.com; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self' ipc: http://ipc.localhost"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["deb", "appimage", "msi", "app", "dmg"],
    "icons": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "publisher": "Miguel50Flowers",
    "category": "DeveloperTool",
    "shortDescription": "Automated daily standup generator",
    "longDescription": "autostand compiles daily standups from git, GitHub, Jira, notes, and AI coding tooling conversations."
  },
  "plugins": {
    "fs": {
      "scope": [
        "$HOME/.claude/**",
        "$HOME/.codex/**",
        "$HOME/.gemini/**",
        "$HOME/.local/share/opencode/**",
        "$HOME/Documents/Github/**",
        "$HOME/Sync/Github_Dailies/**"
      ]
    },
    "shell": {
      "open": true,
      "sidecar": false,
      "scope": [
        { "name": "claude", "command": "claude", "args": true },
        { "name": "ollama", "command": "ollama", "args": true },
        { "name": "codex",  "command": "codex",  "args": true },
        { "name": "gemini", "command": "gemini", "args": true },
        { "name": "grok",   "command": "grok",   "args": true },
        { "name": "gh",     "command": "gh",      "args": true },
        { "name": "git",    "command": "git",     "args": true }
      ]
    },
    "store": {
      "path": "config.json"
    },
    "stronghold": {
      "path": "autostand.vault"
    }
  }
}
```

---

## `src-tauri/src/main.rs`

Thin binary entrypoint — all logic lives in the library crate so it can be reused by tests and mobile targets.

```rust
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() {
    autostand_lib::run()
}
```

For mobile targets the entrypoint is declared via the `tauri::mobile_entry_point` macro:

```rust
#![cfg_attr(mobile, tauri::mobile_entry_point)]
fn main() {
    autostand_lib::run()
}
```

---

## `src-tauri/src/lib.rs`

```rust
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_stronghold::Builder::new(|_app| {
            Box::pin(async { Ok(()) })
        }).build())
        .setup(|app| {
            autostand_scheduler::install_if_missing(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            autostand_app::commands::get_config,
            autostand_app::commands::set_config,
            autostand_app::commands::get_host_slug,
            autostand_app::commands::set_host_slug,
            autostand_app::commands::list_data_sources,
            autostand_app::commands::toggle_data_source,
            autostand_app::commands::list_llm_providers,
            autostand_app::commands::test_llm_provider,
            autostand_app::commands::compile_standup,
            autostand_app::commands::compile_all,
            autostand_app::commands::read_standup_file,
            autostand_app::commands::add_manual_item,
            autostand_app::commands::list_audit_sidecars,
            autostand_app::commands::read_audit_sidecar,
            autostand_app::commands::get_pipeline_status,
            autostand_app::commands::preview_gather,
            autostand_app::commands::get_scheduler_status,
            autostand_app::commands::set_scheduler_schedule,
            autostand_app::commands::trigger_run_now,
            autostand_app::commands::discover_repos,
            autostand_app::commands::get_settings_paths,
            autostand_app::commands::validate_paths,
            autostand_app::commands::store_api_key,
            autostand_app::commands::get_api_key_status,
            autostand_app::commands::detect_cli,
        ])
        .run(tauri::generate_context!())
        .expect("error while running autostand");
}
```

A typical `commands` module lives at `src-tauri/src/commands/mod.rs` and re-exports one file per concern:

```
src-tauri/src/
├── main.rs
├── lib.rs
├── state.rs          # AppState (config, scheduler handle, pipeline lock)
└── commands/
    ├── mod.rs
    ├── config.rs
    ├── host.rs
    ├── data_sources.rs
    ├── llm.rs
    ├── compile.rs
    ├── standup_file.rs
    ├── audit.rs
    ├── pipeline.rs
    ├── scheduler.rs
    ├── paths.rs
    └── secrets.rs
```

---

## `src-tauri/capabilities/`

Tauri v2 uses JSON capability files to grant permissions to windows. Each capability maps a window label to a set of permission identifiers.

`src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capability for the main autostand window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "fs:allow-read-text-file",
    "fs:allow-write-text-file",
    "fs:allow-create-dir",
    "fs:allow-remove-file",
    "fs:allow-read-dir",
    {
      "identifier": "fs:allow-read-text-file",
      "allow": [
        { "path": "$HOME/.claude/**" },
        { "path": "$HOME/.codex/**" },
        { "path": "$HOME/.gemini/**" },
        { "path": "$HOME/.local/share/opencode/**" },
        { "path": "$HOME/.config/opencode/**" },
        { "path": "$HOME/Documents/Github/**" },
        { "path": "$HOME/Sync/Github_Dailies/**" }
      ]
    },
    {
      "identifier": "fs:allow-write-text-file",
      "allow": [
        { "path": "$HOME/Sync/Github_Dailies/**" },
        { "path": "$HOME/Documents/Github/**/dailies/**" },
        { "path": "$APPDATA/**" },
        { "path": "$APPLOCALDATA/**" }
      ]
    },
    "store:default",
    "shell:allow-execute",
    {
      "identifier": "shell:allow-execute",
      "allow": [
        { "name": "claude", "command": "claude", "args": true },
        { "name": "ollama", "command": "ollama", "args": true },
        { "name": "codex",  "command": "codex",  "args": true },
        { "name": "gemini", "command": "gemini", "args": true },
        { "name": "grok",   "command": "grok",   "args": true },
        { "name": "gh",     "command": "gh",      "args": true },
        { "name": "git",    "command": "git",     "args": true }
      ]
    },
    "dialog:allow-open",
    "dialog:allow-save",
    "notification:default",
    "stronghold:default"
  ]
}
```

> Tauri v2 merges `capabilities/*.json` at build time. Splitting into `default.json` + `scheduler.json` (for the headless scheduler window, if any) is supported.

---

## Frontend Vite config

`apps/autostand-app/vite.config.ts`:

```ts
import { defineConfig } from "vite";
import react    from "@vitejs/plugin-react";
import tailwind from "@tailwindcss/vite";
import path    from "node:path";

// Tauri sets these env vars; fallback to 1420 for standalone `pnpm dev`.
const host = process.env.TAURI_DEV_HOST ?? "localhost";
const port = Number(process.env.TAURI_DEV_PORT ?? 1420);

export default defineConfig(async () => ({
  plugins: [react(), tailwind()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "./src") },
  },
  clearScreen: false,
  server: {
    host,
    port,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
    proxy: {},
  },
  build: {
    target: "esnext",
    outDir: "dist",
    emptyOutDir: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
}));
```

`tsconfig.json` (paths):

```jsonc
{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "baseUrl": ".",
    "paths": { "@/*": ["./src/*"] }
  },
  "include": ["src"]
}
```

---

## Dev commands

| Command | What it does |
| --- | --- |
| `pnpm tauri dev` | Hot-reload both Vite (frontend) and Rust (`cargo run`) |
| `pnpm tauri build` | Produce release bundles for the current OS |
| `pnpm tauri build --target aarch64-apple-darwin` | Cross-build (requires target installed via `rustup`) |
| `cargo test --workspace` | Run all unit + integration tests across crates |
| `pnpm storybook` | Storybook for the React component library (shares `design-system/tokens/`) |
| `pnpm lint` | ESLint + `cargo clippy --workspace` |
| `pnpm typecheck` | `tsc --noEmit` for frontend |

`package.json` scripts (excerpt):

```jsonc
{
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "tauri": "tauri",
    "storybook": "storybook dev -p 6006",
    "lint": "eslint src && cargo clippy --workspace",
    "typecheck": "tsc --noEmit"
  }
}
```

---

## Platform-specific notes

| Concern | macOS | Linux | Windows |
| --- | --- | --- | --- |
| Distribution bundle | `.app` + `.dmg` (codesign + notarize via `xcrun notarytool`) | `.deb`, `.AppImage`, `.rpm` | `.msi` (WiX) or `.exe` (NSIS) |
| Codesigning | Required for notarized distribution: `tauri.conf.json` → `bundle.macOS.signingIdentity` + `entitlements` | N/A (consider `gpg` signing for `.deb`) | Authenticode optional; SmartScreen warns unsigned MSI |
| Host slug source | `scutil --get LocalHostName` | `hostnamectl --static` or `/etc/hostname` (strip domain) | `GetComputerNameW` via `windows`/`gethostname` crate |
| Scheduler install path | `~/Library/LaunchAgents/com.miguel50flowers.autostand.plist` | `~/.config/systemd/user/autostand.{service,timer}` | `schtasks /Create /XML autostand.xml /tn autostand` |
| Config dir (`dirs` crate) | `~/Library/Application Support/autostand` | `$XDG_CONFIG_HOME/autostand` → `~/.config/autostand` | `%APPDATA%\autostand` |
| State dir | `~/Library/Application Support/autostand/state` | `~/.local/share/autostand/state` | `%LOCALAPPDATA%\autostand\state` |
| Default webview | WKWebView (system) | WebKitGTK (`webkit2gtk-4.1`) | WebView2 (Edge runtime) |
| Tray icon | Supported; bundle `icons/icon.icns` | Supported; needs `libayatana-appindicator3` | Supported; bundle `icons/icon.ico` |
| Notarization | `xcrun notarytool submit <dmg> --apple-id <id> --team-id <team> --wait` then `xcrun stapler staple` | N/A | N/A |
| Installer postinstall | N/A | `dpkg-deb`/`appimage` metadata | NSIS `!insertmacro` or WiX custom action |