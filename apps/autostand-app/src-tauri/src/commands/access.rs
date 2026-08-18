//! What the OS will actually let this app read, and how to fix it when it won't.
//!
//! autostand reads a lot of a developer's home directory: the dailies folder,
//! the repositories it scans, and the credential each provider CLI wrote. On
//! macOS most of that is behind TCC — the first time a process touches
//! `~/Documents`, `~/Desktop`, `~/Downloads` or an iCloud Drive folder, the
//! system either shows a consent dialog or, for a background probe, denies it
//! silently. A denial is an ordinary `PermissionDenied`, which the pipeline
//! would otherwise report as "no repositories found": the same message an empty
//! folder produces, and the wrong thing to do about it.
//!
//! So this reports access as its own answer, separately from configuration.
//!
//! # What can and cannot be requested
//!
//! There is no API to *ask* for Files and Folders access. macOS shows its
//! dialog when a process touches a protected location, and only once per app per
//! location — after that a denial is remembered and only Settings can undo it.
//! [`request_system_access`] therefore does exactly what the OS responds to: it
//! touches each location, which either raises the native prompt or returns the
//! remembered denial. Where that is not enough, the remediation is a deep link
//! into the right Settings pane, because a user cannot be walked there in words.
//!
//! Windows and Linux gate none of this. Their statuses come back `Granted`
//! without a probe, and the only thing either has to ask for is notifications,
//! which the notification plugin already owns.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::commands::{load_config, repos::resolve_github_dir, standup::resolve_dailies_dir};
use crate::error::AppError;

/// Whether the app can read one location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccessState {
    /// Readable right now.
    Granted,
    /// The OS refused. Only the user can change this, in Settings.
    Denied,
    /// Nothing is there to read. Not a permission problem, and not this
    /// command's business to fix — `get_standup_readiness` owns that.
    Missing,
    /// This OS does not gate the location at all.
    NotApplicable,
}

/// One location the app needs, and what the OS said about it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessCheck {
    /// Stable identifier the UI keys on. Never localized.
    pub id: String,
    /// What this location is, in the user's terms.
    pub label: String,
    /// Why autostand reads it. Shown next to a denial, where the honest
    /// question is "why does a standup tool want my Documents folder".
    pub reason: String,
    pub path: String,
    pub state: AccessState,
}

/// The whole picture, plus whether anything needs the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemAccess {
    /// `"macos"`, `"windows"` or `"linux"` — the UI phrases its guidance per OS.
    pub platform: String,
    /// True when this OS gates filesystem access at all.
    pub gated: bool,
    pub checks: Vec<AccessCheck>,
    /// True when at least one check is `Denied`.
    pub needs_attention: bool,
    /// Deep link to the Settings pane that grants access, when one exists.
    pub settings_url: Option<String>,
}

/// Report what the app can read, without changing anything.
#[tauri::command]
pub async fn get_system_access(app: AppHandle) -> Result<SystemAccess, AppError> {
    Ok(summarize(locations(&app).into_iter().map(|location| {
        let state = probe(&location.path);
        AccessCheck {
            id: location.id,
            label: location.label,
            reason: location.reason,
            path: location.path.display().to_string(),
            state,
        }
    })))
}

/// Touch each location so the OS raises its consent dialog, then re-report.
///
/// On macOS the returned status is what the user chose, because the probe blocks
/// until the dialog is answered. On a location that was already refused once,
/// macOS answers from its record without showing anything — which is why the
/// result carries `settings_url` for the caller to offer next.
#[tauri::command]
pub async fn request_system_access(app: AppHandle) -> Result<SystemAccess, AppError> {
    get_system_access(app).await
}

/// Open the Settings pane where the user can grant what was denied.
///
/// Takes no argument on purpose. The obvious shape — an `open_url(url)` the
/// frontend calls with `settings_url` — would be a command that opens whatever
/// it is handed, reachable from any code running in the webview. The one URL
/// this is allowed to open is compiled in.
#[tauri::command]
pub async fn open_access_settings() -> Result<(), AppError> {
    let Some(url) = settings_url() else {
        return Err(AppError::Invalid(format!(
            "{} has no access settings pane",
            platform()
        )));
    };
    tracing::info!("opening the OS privacy settings");
    tauri_plugin_opener::open_url(url, None::<&str>)
        .map_err(|e| AppError::Io(format!("open access settings: {e}")))
}

/// A location the app reads.
struct Location {
    id: String,
    label: String,
    reason: String,
    path: PathBuf,
}

/// Everything worth checking, resolved against the current configuration.
fn locations(app: &AppHandle) -> Vec<Location> {
    // The same resolvers Settings reports from, so what this command checks and
    // what the pipeline opens cannot drift apart.
    let config = load_config(app).ok();
    let home = dirs::home_dir();
    let mut out = vec![
        Location {
            id: "dailies-dir".into(),
            label: "Standup folder".into(),
            reason: "Where your dated standup files are written and read back.".into(),
            path: resolve_dailies_dir(
                config.as_ref().map(|c| c.dailies_dir.as_str()),
                home.as_deref(),
            ),
        },
        Location {
            id: "github-dir".into(),
            label: "Repository folder".into(),
            reason: "Scanned for git repositories, to read the commits you authored.".into(),
            path: resolve_github_dir(
                config.as_ref().map(|c| c.github_dir.as_str()),
                home.as_deref(),
            ),
        },
    ];

    // The provider credential directories, which is where a "usage unavailable"
    // that is really a permission denial comes from.
    if let Some(home) = home {
        for (id, label, dir) in [
            ("claude-credentials", "Claude CLI login", ".claude"),
            ("codex-credentials", "Codex CLI login", ".codex"),
        ] {
            out.push(Location {
                id: id.into(),
                label: label.into(),
                reason: "Read-only, to report how much quota that provider says is left.".into(),
                path: home.join(dir),
            });
        }
    }
    out
}

/// Read `path` the same way the pipeline would, and classify what happened.
///
/// A directory listing rather than a `metadata` call: on macOS `metadata`
/// succeeds for a TCC-protected directory and only the read is refused, so a
/// stat-based check reports access the app does not have.
fn probe(path: &Path) -> AccessState {
    if !cfg!(target_os = "macos") {
        return AccessState::NotApplicable;
    }
    match fs::read_dir(path) {
        Ok(mut entries) => {
            // `read_dir` can succeed and the first `next()` still fail: the
            // refusal arrives when the directory is actually read.
            match entries.next() {
                Some(Err(err)) if err.kind() == ErrorKind::PermissionDenied => AccessState::Denied,
                _ => AccessState::Granted,
            }
        }
        Err(err) => match err.kind() {
            ErrorKind::PermissionDenied => AccessState::Denied,
            ErrorKind::NotFound => AccessState::Missing,
            _ => AccessState::Granted,
        },
    }
}

/// Collapse the checks into the answer the UI acts on.
fn summarize(checks: impl Iterator<Item = AccessCheck>) -> SystemAccess {
    let checks: Vec<AccessCheck> = checks.collect();
    let needs_attention = checks.iter().any(|c| c.state == AccessState::Denied);
    SystemAccess {
        platform: platform().into(),
        gated: cfg!(target_os = "macos"),
        needs_attention,
        settings_url: settings_url().map(str::to_owned),
        checks,
    }
}

const fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

/// The Settings pane that grants what was denied.
const fn settings_url() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        // Files and Folders, not Full Disk Access: this app needs specific
        // folders, and sending someone to the larger permission to solve a
        // smaller problem is how a tool ends up with access it never needed.
        Some("x-apple.systempreferences:com.apple.preference.security?Privacy_FilesAndFolders")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn check(id: &str, state: AccessState) -> AccessCheck {
        AccessCheck {
            id: id.into(),
            label: id.into(),
            reason: String::new(),
            path: String::new(),
            state,
        }
    }

    #[test]
    fn a_readable_directory_is_granted_or_ungated() {
        let dir = TempDir::new().unwrap();
        let state = probe(dir.path());
        assert!(
            matches!(state, AccessState::Granted | AccessState::NotApplicable),
            "a directory this process just created must be readable: {state:?}",
        );
    }

    #[test]
    fn an_absent_directory_is_missing_not_denied() {
        let dir = TempDir::new().unwrap();
        let state = probe(&dir.path().join("nothing-here"));
        assert!(
            matches!(state, AccessState::Missing | AccessState::NotApplicable),
            "an empty configuration must not be reported as a permission problem, \
             which would send the user to System Settings to fix a path: {state:?}",
        );
    }

    #[test]
    fn only_a_denial_asks_for_the_user() {
        let quiet = summarize(
            [
                check("a", AccessState::Granted),
                check("b", AccessState::Missing),
                check("c", AccessState::NotApplicable),
            ]
            .into_iter(),
        );
        assert!(!quiet.needs_attention);

        let loud = summarize(
            [
                check("a", AccessState::Granted),
                check("b", AccessState::Denied),
            ]
            .into_iter(),
        );
        assert!(loud.needs_attention);
    }

    #[test]
    fn a_deep_link_exists_exactly_where_the_os_gates_access() {
        let summary = summarize(std::iter::empty());
        assert_eq!(
            summary.gated,
            summary.settings_url.is_some(),
            "offering a Settings button on an OS with no such pane is a dead end, \
             and gating access with no way to grant it is worse",
        );
    }
}
