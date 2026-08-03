# Platform Targets

`autostand` ships as a native desktop app on macOS, Linux, and Windows. This document fixes the bundle formats, host-slug derivation, scheduler installation, CLI binary discovery, config/state paths, and file permissions for each platform.

---

## Bundle formats

| Platform | Bundles | Toolchain | Codesigning |
| --- | --- | --- | --- |
| macOS | `.app`, `.dmg` | `tauri build` → `target/release/bundle/{macos,dmg}` | Required for distribution: codesign with Developer ID + notarize via `xcrun notarytool` + staple |
| Linux | `.deb`, `.AppImage`, `.rpm` | `tauri build` → `target/release/bundle/{deb,appimage,rpm}` | Optional: `gpg --detach-sign` for `.deb`; AppImage signs via zsync |
| Windows | `.msi` (WiX), `.exe` (NSIS) | `tauri build` → `target/release/bundle/{msi,nsis}` | Optional: Authenticode sign with `signtool` |

### Tauri v2 bundle config

`tauri.conf.json`:

```jsonc
{
  "bundle": {
    "active": true,
    "targets": ["deb", "appimage", "msi", "app", "dmg"],
    "icons": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "macOS": {
      "signingIdentity": "Developer ID Application: Miguel50Flowers (TEAMID)",
      "entitlements": "entitlements.plist",
      "minimumSystemVersion": "11.0"
    },
    "windows": {
      "wix": { "language": "en-US" },
      "nsis": { "installMode": "perMachine" }
    },
    "linux": {
      "deb":  { "depends": ["libwebkit2gtk-4.1-0", "libgtk-3-0", "libayatana-appindicator3-1"] },
      "appimage": { "bundleMediaFramework": false }
    }
  }
}
```

To build for a single target:

```bash
pnpm tauri build --target aarch64-apple-darwin
pnpm tauri build --target x86_64-pc-windows-msi
```

---

## Host slug derivation per platform

The host slug identifies which machine wrote an AUTO block. It must be **stable**, **human-readable**, and **never** derived from a DHCP/IP address. The slug is detected on first run, validated, and persisted to `state/host-id`. After that, the persisted value is always used.

| Platform | Source command | Crate/API | Validation | Persist to |
| --- | --- | --- | --- | --- |
| macOS | `scutil --get LocalHostName` | `std::process::Command` | reject if numeric or IP-like (`^\d+$` or `^\d+\.\d+\.\d+\.\d+$`) | `state/host-id` |
| Linux | `hostnamectl --static`, fallback `/etc/hostname` | `hostname` crate (`hostname::get`) | strip domain (first segment of FQDN); reject numeric/IP | `state/host-id` |
| Windows | `GetComputerNameW` Win32 API | `windows` crate or `gethostname` crate | reject if numeric | `state/host-id` |
| Cross-platform fallback | `dirs::hostname()` or `hostname::get().unwrap().into_string()` | `hostname` crate | reject numeric/IP | `state/host-id` |

### Rust implementation

```rust
// crates/autostand-core/src/host.rs
use anyhow::{Context, Result};

pub fn detect() -> Result<String> {
    let raw = detect_raw()?;
    let slug = sanitize(&raw);
    if slug.is_empty() || slug.chars().all(char::is_numeric) || is_ip_like(&slug) {
        anyhow::bail!("invalid host slug detected: {raw:?}");
    }
    Ok(slug)
}

#[cfg(target_os = "macos")]
fn detect_raw() -> Result<String> {
    let out = std::process::Command::new("scutil")
        .arg("--get").arg("LocalHostName")
        .output().context("scutil --get LocalHostName")?;
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

#[cfg(target_os = "linux")]
fn detect_raw() -> Result<String> {
    // Prefer hostnamectl, fall back to /etc/hostname, then hostname crate.
    if let Ok(out) = std::process::Command::new("hostnamectl")
        .arg("--static").output() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() { return Ok(s.split('.').next().unwrap_or(&s).to_string()); }
    }
    hostname::get().context("hostname::get")?.into_string().map_err(Into::into)
}

#[cfg(target_os = "windows")]
fn detect_raw() -> Result<String> {
    // gethostname crate wraps GetComputerNameW on Windows.
    hostname::get().context("hostname::get")?.into_string().map_err(Into::into)
}

fn is_ip_like(s: &str) -> bool {
    s.split('.').filter(|p| p.parse::<u8>().is_ok()).count() == 4
}

pub fn detect_or_load(state_dir: &Path) -> Result<String> {
    let path = state_dir.join("host-id");
    if let Ok(s) = std::fs::read_to_string(&path) {
        let s = s.trim().to_string();
        if !s.is_empty() && !s.chars().all(char::is_numeric) && !is_ip_like(&s) {
            return Ok(s);
        }
    }
    let slug = detect()?;
    std::fs::create_dir_all(state_dir)?;
    std::fs::write(&path, &slug)?;
    Ok(slug)
}
```

---

## Scheduler per platform

`autostand-scheduler` can install a system scheduler on first run (preferred) or fall back to an in-process `tokio` cron task while the app is open.

| Platform | System scheduler | Unit file path | Install command | Tauri install hook |
| --- | --- | --- | --- | --- |
| macOS | launchd | `~/Library/LaunchAgents/com.miguel50flowers.autostand.plist` | write plist; `launchctl load` | on `setup`, call `scheduler::install_if_missing(app)` |
| Linux | systemd user | `~/.config/systemd/user/autostand.service` + `autostand.timer` | `systemctl --user enable --now autostand.timer` | same |
| Windows | Task Scheduler | `autostand.xml` (definition) | `schtasks /Create /XML autostand.xml /tn autostand /F` | same |
| Fallback (all) | in-process `tokio::cron` | n/a (lives in `AppState`) | runs only while the app process is alive | always available |

### launchd plist (macOS)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.miguel50flowers.autostand</string>
  <key>ProgramArguments</key>
  <array>
    <string>/Applications/autostand.app/Contents/MacOS/autostand</string>
    <string>--run</string>
  </array>
  <key>StartCalendarInterval</key>
  <array>
    <!-- hourly 07-19 weekdays -->
    <dict><key>Hour</key><integer>7</integer><key>Weekday</key><integer>1-5</integer></dict>
    <dict><key>Hour</key><integer>8</integer></dict>
    <dict><key>Hour</key><integer>9</integer></dict>
    <dict><key>Hour</key><integer>10</integer></dict>
    <dict><key>Hour</key><integer>11</integer></dict>
    <dict><key>Hour</key><integer>12</integer></dict>
    <dict><key>Hour</key><integer>13</integer></dict>
    <dict><key>Hour</key><integer>14</integer></dict>
    <dict><key>Hour</key><integer>15</integer></dict>
    <dict><key>Hour</key><integer>16</integer></dict>
    <dict><key>Hour</key><integer>17</integer></dict>
    <dict><key>Hour</key><integer>18</integer></dict>
    <dict><key>Hour</key><integer>19</integer></dict>
  </array>
  <key>RunAtLoad</key><false/>
  <key>StandardOutPath</key><string>/tmp/autostand.out.log</string>
  <key>StandardErrorPath</key><string>/tmp/autostand.err.log</string>
</dict>
</plist>
```

### systemd user unit (Linux)

`~/.config/systemd/user/autostand.service`:

```ini
[Unit]
Description=autostand daily-standup compiler

[Service]
Type=oneshot
ExecStart=%h/.local/bin/autostand --run
```

`~/.config/systemd/user/autostand.timer`:

```ini
[Unit]
Description=Run autostand hourly on weekdays 07-19

[Timer]
OnCalendar=Mon..Fri 07..19:00:00
Persistent=true

[Install]
WantedBy=timers.target
```

### Windows Task Scheduler XML

```xml
<?xml version="1.0" encoding="UTF-16"?>
<Task xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>2026-01-01T07:00:00</StartBoundary>
      <ScheduleByWeek>
        <WeeksInterval>1</WeeksInterval>
        <DaysOfWeek>
          <Monday/><Tuesday/><Wednesday/><Thursday/><Friday/>
        </DaysOfWeek>
      </ScheduleByWeek>
      <Repetition>
        <Interval>PT1H</Interval>
        <Duration>PT13H</Duration>
      </Repetition>
    </CalendarTrigger>
  </Triggers>
  <Actions>
    <Exec>
      <Command>%LOCALAPPDATA%\autostand\autostand.exe</Command>
      <Arguments>--run</Arguments>
    </Exec>
  </Actions>
</Task>
```

Install:

```powershell
schtasks /Create /XML autostand.xml /tn autostand /F
```

### In-process fallback (all platforms)

```rust
// crates/autostand-scheduler/src/in_process.rs
pub fn spawn(app: tauri::AppHandle, cron: String) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut sched = tokio_cron_scheduler::JobScheduler::new().await.unwrap();
        let app2 = app.clone();
        sched.add(
            Job::new_async(cron.as_str(), move |_, _| {
                let app3 = app2.clone();
                Box::pin(async move {
                    let _ = autostand_core::pipeline::trigger(
                        autostand_core::TriggerSource::Scheduled, &app3).await;
                })
            }).unwrap()
        ).await.unwrap();
        sched.start().await.unwrap();
    })
}
```

---

## CLI binary discovery per platform

CLIs (`claude`, `ollama`, `codex`, `gemini`, `grok`, `gh`, `git`) are discovered via `PATH` first. If `PATH` lookup fails, `autostand-adapters::cli::detect` falls back to platform-specific default install locations.

| CLI | macOS defaults | Linux defaults | Windows defaults |
| --- | --- | --- | --- |
| `claude` | `/opt/homebrew/bin/claude`, `~/.bun/bin/claude`, `~/.npm-global/bin/claude` | `~/.local/bin/claude`, `~/.bun/bin/claude` | `%USERPROFILE%\.bun\claude.exe`, `%LOCALAPPDATA%\npm\claude.cmd` |
| `ollama` | `/opt/homebrew/bin/ollama`, `/usr/local/bin/ollama` | `/usr/bin/ollama`, `/usr/local/bin/ollama` | `%PROGRAMFILES%\Ollama\ollama.exe` |
| `codex` | `/opt/homebrew/bin/codex`, `~/.bun/bin/codex` | `~/.local/bin/codex`, `~/.bun/bin/codex` | `%USERPROFILE%\.bun\codex.exe`, `%LOCALAPPDATA%\npm\codex.cmd` |
| `gemini` | `/opt/homebrew/bin/gemini`, `~/.bun/bin/gemini` | `~/.local/bin/gemini`, `~/.bun/bin/gemini` | `%USERPROFILE%\.bun\gemini.exe`, `%LOCALAPPDATA%\npm\gemini.cmd` |
| `grok` | `/opt/homebrew/bin/grok`, `~/.bun/bin/grok` | `~/.local/bin/grok`, `~/.bun/bin/grok` | `%USERPROFILE%\.bun\grok.exe`, `%LOCALAPPDATA%\npm\grok.cmd` |
| `gh` | `/opt/homebrew/bin/gh`, `/usr/local/bin/gh` | `/usr/bin/gh`, `/usr/local/bin/gh` | `%PROGRAMFILES%\GitHub CLI\gh.exe` |
| `git` | `/usr/bin/git`, `/opt/homebrew/bin/git` | `/usr/bin/git`, `/usr/local/bin/git` | `%PROGRAMFILES%\Git\cmd\git.exe` |

### Discovery algorithm

```rust
// crates/autostand-adapters/src/cli.rs
pub struct CliInfo { pub found: bool, pub path: String, pub version: String }

pub fn detect(name: &str) -> CliInfo {
    // 1. Try PATH.
    if let Some(p) = which::which(name).ok() {
        return finalize(name, p);
    }
    // 2. Try platform defaults.
    for candidate in platform_defaults(name) {
        if candidate.exists() {
            return finalize(name, &candidate);
        }
    }
    CliInfo { found: false, path: String::new(), version: String::new() }
}

fn finalize(name: &str, path: &Path) -> CliInfo {
    let version = std::process::Command::new(path)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    CliInfo { found: true, path: path.display().to_string(), version }
}
```

---

## Config/state paths per platform

All paths use the `dirs` crate (`dirs::config_dir`, `dirs::data_local_dir`, `dirs::home_dir`) so they respect XDG on Linux and roaming/local split on Windows.

| Path | macOS | Linux | Windows |
| --- | --- | --- | --- |
| Config dir | `~/Library/Application Support/autostand` | `$XDG_CONFIG_HOME/autostand` (default `~/.config/autostand`) | `%APPDATA%\autostand` (roaming) |
| State dir | `~/Library/Application Support/autostand/state` | `$XDG_DATA_HOME/autostand/state` (default `~/.local/share/autostand/state`) | `%LOCALAPPDATA%\autostand\state` |
| Audit dir | `<state>/audit/` | `<state>/audit/` | `<state>\audit\` |
| Dailies output (default) | `<GITHUB_DIR>/dailies/` (configurable) | `<GITHUB_DIR>/dailies/` | `<GITHUB_DIR>\dailies\` |
| Scheduler unit file | `~/Library/LaunchAgents/com.miguel50flowers.autostand.plist` | `~/.config/systemd/user/autostand.{service,timer}` | Task Scheduler (`schtasks` store) |
| Logs | `~/Library/Logs/autostand/` (via `tracing-appender` if enabled) | `<state>/logs/` | `%LOCALAPPDATA%\autostand\logs\` |
| Keychain entry | `autostand.<provider>` (macOS Keychain) | Secret Service (`gnome-keyring`/`kwallet`) | Windows Credential Manager |

### Rust resolution

```rust
use dirs::{config_dir, data_local_dir, home_dir};

pub fn config_dir_for() -> PathBuf { config_dir().unwrap().join("autostand") }
pub fn state_dir_for()  -> PathBuf { data_local_dir().unwrap().join("autostand").join("state") }
pub fn audit_dir_for()  -> PathBuf { state_dir_for().join("audit") }
pub fn default_github_dir() -> Option<PathBuf> { home_dir().map(|h| h.join("Documents").join("Github")) }
pub fn default_dailies_dir() -> Option<PathBuf> {
    default_github_dir().map(|g| g.join("dailies"))
}
```

---

## File permissions

| File | On-disk mode (current machine) | Git-stored mode | Rationale |
| --- | --- | --- | --- |
| Standup file (`<date>.md`) | `0600` (owner read/write) | `0644` (committed) | Contains only work summaries; tighter than git because local writes go through atomic temp file |
| Audit sidecar (`state/audit/<date>-<host>.json`) | `0600` | not committed | May contain redacted conversation digests; never shared |
| Config (`config.json`) | `0644` | not committed | No secrets (API keys live in keychain); readable so users can hand-edit |
| `state/host-id` | `0600` | not committed | Per-machine identifier |
| Scheduler unit files | `0644` | not committed | Owned by user; standard for launchd/systemd user units |
| Keychain entries | managed by OS | n/a | Encrypted at rest by the OS |

### Why `0600` on standup files but `0644` in git?

The atomic-write path uses `tempfile::NamedTempFile` (default `0600`) then `persist()` (rename). Git stores the file as `0644` because `.gitattributes` only affects merge behavior, not on-disk mode after checkout. The mismatch is harmless: the file is committed as world-readable content (which is fine — it's a standup summary), but the local working copy keeps `0600` after our write because we never `chmod` it back.

### `.gitattributes` (in the dailies repo)

```gitattributes
20YY-MM-DD.md merge=union
```

The `union` merge driver concatenates both sides of a conflict instead of emitting `<<<<<<<` markers. This is what lets two machines append AUTO blocks for different hosts to the same date file without manual resolution. See `docs/specs/standup-file-format.md` for the full block grammar.