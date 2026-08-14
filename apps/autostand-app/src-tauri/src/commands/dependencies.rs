//! Shared prerequisite probes and guided remediation.
//!
//! Repo Sync and Local AI both depend on programs Autostand does not implement:
//! `git`/`gh` on one side, the `autostand-local-llm` sidecar plus a llama.cpp
//! runtime on the other. Every probe lives here so both features answer from the
//! same evidence, and so an unmet prerequisite always carries the one next step
//! that fixes it instead of a dead "Missing" chip.
//!
//! Two rules shape the DTOs:
//!
//! 1. No field ever carries a subprocess's stdout or stderr. A failing
//!    `gh auth status` prints credential diagnostics, and `docs/specs/audit.md`
//!    already forbids that shape of leak; the same criterion applies here.
//! 2. A remediation is only offered when it can be vouched for. If no package
//!    manager on this machine has a package id we know exists, the answer is an
//!    honest documentation link rather than a command that fails.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::process::Command;

use crate::error::AppError;

/// Feature whose external prerequisites are reported together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyGroup {
    RepoSync,
    LocalAi,
}

/// How one prerequisite is doing on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyState {
    Ok,
    Missing,
    /// Installed, but not usable as configured: signed out, nothing selected.
    Misconfigured,
    /// Not answerable yet, because the check itself needs something missing.
    Unknown,
}

/// What kind of next step the user is being offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemediationKind {
    /// A shell command. Always shown verbatim before anything may run it.
    TerminalCommand,
    /// Something the user completes in this very screen.
    InAppAction,
    /// Official documentation, used whenever no command can be vouched for.
    DocLink,
}

/// The single next step for one unmet dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remediation {
    pub kind: RemediationKind,
    pub label: String,
    /// Exact command, so the user reads it before deciding to run it.
    pub command: Option<String>,
    pub url: Option<String>,
    /// Whether [`run_dependency_remediation`] may execute this itself. False for
    /// anything needing elevation, a browser handshake, or an interactive shell.
    pub runnable: bool,
    pub note: Option<String>,
}

/// One prerequisite, its state, and the step that would satisfy it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub id: String,
    pub group: DependencyGroup,
    pub label: String,
    pub description: String,
    pub state: DependencyState,
    /// Curated detail — a resolved path or a short reason. Never command output.
    pub detail: Option<String>,
    /// `None` exactly when the dependency is satisfied.
    pub remediation: Option<Remediation>,
}

/// Result of a remediation the user explicitly asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemediationOutcome {
    pub dependency_id: String,
    /// False when the step is the user's to take, so the UI does not claim work
    /// the app did not do.
    pub performed: bool,
    pub message: String,
    /// The same dependency, re-probed after the step.
    pub dependency: Dependency,
}

pub(crate) const GIT_ID: &str = "repo-sync.git";
pub(crate) const GH_ID: &str = "repo-sync.gh";
pub(crate) const GH_AUTH_ID: &str = "repo-sync.gh-auth";
pub(crate) const SIDECAR_ID: &str = "local-ai.sidecar";
pub(crate) const RUNTIME_ID: &str = "local-ai.runtime";
pub(crate) const MODEL_ID: &str = "local-ai.model";

/// Override honoured by the local sidecar; mirrored so the checklist agrees
/// with what inference will actually use.
const RUNTIME_ENV_OVERRIDE: &str = "AUTOSTAND_LLAMA_CLI";
const LOCAL_PROVIDER_ID: &str = "builtin-local";

const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// A cold Homebrew install downloads a bottle; a build from source is slower.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(900);

// ── Program discovery ─────────────────────────────────────────────────────

/// Resolve an executable without invoking a shell.
///
/// The optional PATH is what makes the probes hermetic in tests; production also
/// probes the common GUI-app locations on macOS, where an app launched from
/// Finder does not inherit the user's interactive shell PATH.
pub(crate) fn find_program(name: &str, path_override: Option<&OsStr>) -> Option<PathBuf> {
    let path = path_override
        .map(OsString::from)
        .or_else(|| std::env::var_os("PATH"));
    let mut dirs = path
        .as_deref()
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    #[cfg(target_os = "macos")]
    if path_override.is_none() {
        dirs.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
        ]);
    }

    for dir in dirs {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        #[cfg(target_os = "windows")]
        for suffix in ["exe", "cmd", "bat"] {
            let candidate = dir.join(format!("{name}.{suffix}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Executable name of the process-isolated local inference sidecar.
const fn sidecar_binary() -> &'static str {
    if cfg!(windows) {
        "autostand-local-llm.exe"
    } else {
        "autostand-local-llm"
    }
}

/// llama.cpp split one-shot completion out of `llama-cli`; the sidecar prefers
/// the dedicated program and keeps the older one as a fallback, so the probe
/// must accept exactly the same pair.
const fn runtime_binaries() -> [&'static str; 2] {
    if cfg!(windows) {
        ["llama-completion.exe", "llama-cli.exe"]
    } else {
        ["llama-completion", "llama-cli"]
    }
}

fn sibling_of(base: &Path, names: &[&str]) -> Option<PathBuf> {
    names
        .iter()
        .map(|name| base.with_file_name(name))
        .find(|candidate| candidate.is_file())
}

/// Mirror of the adapter's own lookup: a configured override that really is a
/// file, then a sibling of the running executable, then PATH.
fn probe_sidecar(configured: Option<&str>, path_override: Option<&OsStr>) -> Option<PathBuf> {
    let override_path = configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_file());
    if override_path.is_some() {
        return override_path;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| sibling_of(&exe, &[sidecar_binary()]))
        .or_else(|| find_program(sidecar_binary(), path_override))
}

/// Release bundles place the runtime beside the sidecar, so that directory is
/// checked before the app's own and before PATH.
fn probe_runtime(
    sidecar: Option<&Path>,
    env_override: Option<&OsStr>,
    path_override: Option<&OsStr>,
) -> Option<PathBuf> {
    let explicit = env_override
        .map(PathBuf::from)
        .filter(|path| path.is_file());
    if explicit.is_some() {
        return explicit;
    }
    sidecar
        .and_then(|path| sibling_of(path, &runtime_binaries()))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| sibling_of(&exe, &runtime_binaries()))
        })
        .or_else(|| {
            runtime_binaries()
                .iter()
                .find_map(|name| find_program(name, path_override))
        })
}

/// Ask `gh` whether it holds a github.com login.
///
/// Every byte of output is dropped rather than captured: an authentication
/// failure prints token and host diagnostics that must not reach a DTO or a log.
pub(crate) async fn gh_authenticated(gh: &Path) -> bool {
    let mut command = Command::new(gh);
    command
        .args(["auth", "status", "--hostname", "github.com"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    matches!(
        tokio::time::timeout(PROBE_TIMEOUT, command.status()).await,
        Ok(Ok(status)) if status.success()
    )
}

// ── Package managers ──────────────────────────────────────────────────────

/// A package manager we found on this machine. Nothing is ever suggested for a
/// manager that is not installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManager {
    Homebrew,
    Winget,
    Apt,
    Dnf,
    Pacman,
    Zypper,
}

impl PackageManager {
    const fn program(self) -> &'static str {
        match self {
            Self::Homebrew => "brew",
            Self::Winget => "winget",
            Self::Apt => "apt",
            Self::Dnf => "dnf",
            Self::Pacman => "pacman",
            Self::Zypper => "zypper",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Homebrew => "Install with Homebrew",
            Self::Winget => "Install with winget",
            Self::Apt => "Install with apt",
            Self::Dnf => "Install with dnf",
            Self::Pacman => "Install with pacman",
            Self::Zypper => "Install with zypper",
        }
    }

    /// The command to show, and whether Autostand may run it unattended.
    ///
    /// Only Homebrew qualifies: everything else needs elevation or answers a
    /// prompt Autostand cannot see, and a hung install is worse than a command
    /// the user pastes into their own terminal.
    fn install_command(self, package: &str) -> (String, bool) {
        match self {
            Self::Homebrew => (format!("brew install {package}"), true),
            Self::Winget => (
                format!("winget install --id {package} -e --source winget"),
                false,
            ),
            Self::Apt => (format!("sudo apt install {package}"), false),
            Self::Dnf => (format!("sudo dnf install {package}"), false),
            Self::Pacman => (format!("sudo pacman -S {package}"), false),
            Self::Zypper => (format!("sudo zypper install {package}"), false),
        }
    }
}

fn detect_package_manager(path_override: Option<&OsStr>) -> Option<PackageManager> {
    let candidates: &[PackageManager] = if cfg!(target_os = "macos") {
        &[PackageManager::Homebrew]
    } else if cfg!(target_os = "windows") {
        &[PackageManager::Winget]
    } else {
        &[
            PackageManager::Apt,
            PackageManager::Dnf,
            PackageManager::Pacman,
            PackageManager::Zypper,
        ]
    };
    candidates
        .iter()
        .copied()
        .find(|manager| find_program(manager.program(), path_override).is_some())
}

/// Package ids per manager. `None` means "no id we can vouch for": the user gets
/// the official install guide instead of a command that would fail.
struct InstallSpec {
    homebrew: Option<&'static str>,
    winget: Option<&'static str>,
    apt: Option<&'static str>,
    dnf: Option<&'static str>,
    pacman: Option<&'static str>,
    zypper: Option<&'static str>,
    doc_url: &'static str,
    doc_label: &'static str,
}

impl InstallSpec {
    const fn package(&self, manager: PackageManager) -> Option<&'static str> {
        match manager {
            PackageManager::Homebrew => self.homebrew,
            PackageManager::Winget => self.winget,
            PackageManager::Apt => self.apt,
            PackageManager::Dnf => self.dnf,
            PackageManager::Pacman => self.pacman,
            PackageManager::Zypper => self.zypper,
        }
    }
}

const GIT_INSTALL: InstallSpec = InstallSpec {
    homebrew: Some("git"),
    winget: Some("Git.Git"),
    apt: Some("git"),
    dnf: Some("git"),
    pacman: Some("git"),
    zypper: Some("git"),
    doc_url: "https://git-scm.com/downloads",
    doc_label: "Open the Git download page",
};

/// The GitHub CLI is not in every distribution's default repositories, and its
/// own guide documents the extra repository step — so apt and zypper get the
/// guide rather than an invented package id.
const GH_INSTALL: InstallSpec = InstallSpec {
    homebrew: Some("gh"),
    winget: Some("GitHub.cli"),
    apt: None,
    dnf: Some("gh"),
    pacman: Some("github-cli"),
    zypper: None,
    doc_url: "https://github.com/cli/cli/blob/trunk/docs/install_linux.md",
    doc_label: "Open the GitHub CLI install guide",
};

/// Only Homebrew ships a llama.cpp package this project can name with
/// confidence. Everywhere else the honest answer is the upstream build guide.
const RUNTIME_INSTALL: InstallSpec = InstallSpec {
    homebrew: Some("llama.cpp"),
    winget: None,
    apt: None,
    dnf: None,
    pacman: None,
    zypper: None,
    doc_url: "https://github.com/ggml-org/llama.cpp",
    doc_label: "Open the llama.cpp install guide",
};

fn install_spec(dependency_id: &str) -> Option<&'static InstallSpec> {
    match dependency_id {
        GIT_ID => Some(&GIT_INSTALL),
        GH_ID => Some(&GH_INSTALL),
        RUNTIME_ID => Some(&RUNTIME_INSTALL),
        _ => None,
    }
}

fn install_remediation(spec: &InstallSpec, manager: Option<PackageManager>) -> Remediation {
    match manager.and_then(|found| spec.package(found).map(|package| (found, package))) {
        Some((manager, package)) => {
            let (command, runnable) = manager.install_command(package);
            Remediation {
                kind: RemediationKind::TerminalCommand,
                label: manager.label().to_string(),
                command: Some(command),
                url: Some(spec.doc_url.to_string()),
                runnable,
                note: if runnable {
                    None
                } else {
                    Some(
                        "Run this in your own terminal: it needs rights or a prompt Autostand cannot answer."
                            .to_string(),
                    )
                },
            }
        }
        None => Remediation {
            kind: RemediationKind::DocLink,
            label: spec.doc_label.to_string(),
            command: None,
            url: Some(spec.doc_url.to_string()),
            runnable: false,
            note: None,
        },
    }
}

fn in_app(label: &str) -> Remediation {
    Remediation {
        kind: RemediationKind::InAppAction,
        label: label.to_string(),
        command: None,
        url: None,
        runnable: false,
        note: None,
    }
}

// ── Dependency assembly ───────────────────────────────────────────────────

fn dependency(
    id: &str,
    group: DependencyGroup,
    label: &str,
    description: &str,
    state: DependencyState,
    detail: Option<String>,
    remediation: Option<Remediation>,
) -> Dependency {
    Dependency {
        id: id.to_string(),
        group,
        label: label.to_string(),
        description: description.to_string(),
        state,
        detail,
        remediation: if state == DependencyState::Ok {
            None
        } else {
            remediation
        },
    }
}

fn path_detail(path: Option<&Path>) -> Option<String> {
    path.map(|value| value.to_string_lossy().to_string())
}

/// What the repo-sync probes found. `authenticated` is `None` when `gh` itself
/// is missing, which is a different answer from "signed out".
#[derive(Debug, Default)]
struct RepoProbe {
    git: Option<PathBuf>,
    gh: Option<PathBuf>,
    authenticated: Option<bool>,
}

fn repo_sync_dependencies(probe: &RepoProbe, manager: Option<PackageManager>) -> Vec<Dependency> {
    let git_state = if probe.git.is_some() {
        DependencyState::Ok
    } else {
        DependencyState::Missing
    };
    let gh_state = if probe.gh.is_some() {
        DependencyState::Ok
    } else {
        DependencyState::Missing
    };
    let (auth_state, auth_detail) = match probe.authenticated {
        Some(true) => (DependencyState::Ok, None),
        Some(false) => (
            DependencyState::Misconfigured,
            Some("No github.com account is signed in.".to_string()),
        ),
        None => (
            DependencyState::Unknown,
            Some("Sign-in can only be checked once the GitHub CLI is installed.".to_string()),
        ),
    };

    vec![
        dependency(
            GIT_ID,
            DependencyGroup::RepoSync,
            "Git",
            "Commits and pushes the standup history from the sync folder.",
            git_state,
            path_detail(probe.git.as_deref()),
            Some(install_remediation(&GIT_INSTALL, manager)),
        ),
        dependency(
            GH_ID,
            DependencyGroup::RepoSync,
            "GitHub CLI",
            "Creates the private repository and verifies it stayed private.",
            gh_state,
            path_detail(probe.gh.as_deref()),
            Some(install_remediation(&GH_INSTALL, manager)),
        ),
        dependency(
            GH_AUTH_ID,
            DependencyGroup::RepoSync,
            "GitHub sign-in",
            "An authenticated github.com account for the GitHub CLI.",
            auth_state,
            auth_detail,
            Some(Remediation {
                kind: RemediationKind::TerminalCommand,
                label: "Sign in with the GitHub CLI".to_string(),
                command: Some("gh auth login --hostname github.com --web".to_string()),
                url: Some("https://cli.github.com/manual/gh_auth_login".to_string()),
                // Sign-in opens a browser and waits for a one-time code, so it
                // belongs in the user's own terminal.
                runnable: false,
                note: Some(
                    "Run this in your own terminal: it opens a browser to finish sign-in."
                        .to_string(),
                ),
            }),
        ),
    ]
}

/// What the Local AI probes found on disk.
#[derive(Debug, Default)]
struct LocalProbe {
    sidecar: Option<PathBuf>,
    runtime: Option<PathBuf>,
    selected: Option<String>,
    installed: Vec<String>,
}

fn model_dependency(probe: &LocalProbe) -> Dependency {
    let selected_installed = probe
        .selected
        .as_deref()
        .is_some_and(|id| probe.installed.iter().any(|installed| installed == id));
    let (state, detail, remediation) = if selected_installed {
        (DependencyState::Ok, probe.selected.clone(), None)
    } else if probe.installed.is_empty() {
        (
            DependencyState::Missing,
            Some("No model has been downloaded yet.".to_string()),
            Some(in_app("Download a model from the list below.")),
        )
    } else {
        (
            DependencyState::Misconfigured,
            Some("A downloaded model is available but none is selected.".to_string()),
            Some(in_app("Choose \"Use model\" on a downloaded model below.")),
        )
    };
    dependency(
        MODEL_ID,
        DependencyGroup::LocalAi,
        "Downloaded model",
        "A verified GGUF model selected for on-device rendering.",
        state,
        detail,
        remediation,
    )
}

fn local_ai_dependencies(probe: &LocalProbe, manager: Option<PackageManager>) -> Vec<Dependency> {
    let sidecar_state = if probe.sidecar.is_some() {
        DependencyState::Ok
    } else {
        DependencyState::Missing
    };
    let runtime_state = if probe.runtime.is_some() {
        DependencyState::Ok
    } else {
        DependencyState::Missing
    };

    vec![
        dependency(
            SIDECAR_ID,
            DependencyGroup::LocalAi,
            "Local inference helper",
            "The autostand-local-llm process that keeps models out of the app process.",
            sidecar_state,
            path_detail(probe.sidecar.as_deref()),
            Some(Remediation {
                kind: RemediationKind::DocLink,
                label: "Reinstall Autostand".to_string(),
                // The sidecar ships inside the bundle; no package manager can
                // supply it, so the only honest fix is a complete install.
                command: None,
                url: Some("https://github.com/MAECLY/autostand/releases/latest".to_string()),
                runnable: false,
                note: Some(
                    "The helper ships with Autostand. A source build produces it with `cargo build --workspace`."
                        .to_string(),
                ),
            }),
        ),
        dependency(
            RUNTIME_ID,
            DependencyGroup::LocalAi,
            "llama.cpp runtime",
            "Runs GGUF models on this device (llama-completion, or the older llama-cli).",
            runtime_state,
            path_detail(probe.runtime.as_deref()),
            Some(Remediation {
                note: Some(format!(
                    "Already have a build? Point {RUNTIME_ENV_OVERRIDE} at its executable."
                )),
                ..install_remediation(&RUNTIME_INSTALL, manager)
            }),
        ),
        model_dependency(probe),
    ]
}

/// Which group owns an id, so remediating one dependency never re-probes the
/// other feature — `gh auth status` is a network call, not a lookup.
fn group_of(id: &str) -> Option<DependencyGroup> {
    match id {
        GIT_ID | GH_ID | GH_AUTH_ID => Some(DependencyGroup::RepoSync),
        SIDECAR_ID | RUNTIME_ID | MODEL_ID => Some(DependencyGroup::LocalAi),
        _ => None,
    }
}

const fn includes(filter: Option<DependencyGroup>, group: DependencyGroup) -> bool {
    match filter {
        None => true,
        Some(selected) => matches!(
            (selected, group),
            (DependencyGroup::RepoSync, DependencyGroup::RepoSync)
                | (DependencyGroup::LocalAi, DependencyGroup::LocalAi)
        ),
    }
}

// ── Probing ───────────────────────────────────────────────────────────────

async fn probe_repo_sync() -> RepoProbe {
    let git = find_program("git", None);
    let gh = find_program("gh", None);
    let authenticated = match gh.as_deref() {
        Some(path) => Some(gh_authenticated(path).await),
        None => None,
    };
    RepoProbe {
        git,
        gh,
        authenticated,
    }
}

fn probe_local_ai(app: &AppHandle, config: &super::types::AppConfig) -> LocalProbe {
    let configured = config
        .llm
        .providers
        .iter()
        .find(|provider| provider.id == LOCAL_PROVIDER_ID)
        .and_then(|provider| provider.cli_path.clone());
    let sidecar = probe_sidecar(configured.as_deref(), None);
    let runtime = probe_runtime(
        sidecar.as_deref(),
        std::env::var_os(RUNTIME_ENV_OVERRIDE).as_deref(),
        None,
    );
    LocalProbe {
        sidecar,
        runtime,
        selected: super::local_models::selected_model_id(app),
        installed: super::local_models::available_model_ids(app),
    }
}

async fn dependencies_for(
    app: &AppHandle,
    group: Option<DependencyGroup>,
) -> Result<Vec<Dependency>, AppError> {
    let manager = detect_package_manager(None);
    let mut out = Vec::new();
    if includes(group, DependencyGroup::RepoSync) {
        out.extend(repo_sync_dependencies(&probe_repo_sync().await, manager));
    }
    if includes(group, DependencyGroup::LocalAi) {
        let config = super::load_config(app)?;
        out.extend(local_ai_dependencies(
            &probe_local_ai(app, &config),
            manager,
        ));
    }
    Ok(out)
}

async fn dependency_by_id(app: &AppHandle, id: &str) -> Result<Dependency, AppError> {
    let group = group_of(id).ok_or_else(|| unknown_dependency(id))?;
    dependencies_for(app, Some(group))
        .await?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| unknown_dependency(id))
}

fn unknown_dependency(id: &str) -> AppError {
    AppError::NotFound(format!("unknown dependency: {id}"))
}

/// Install one Homebrew package, discarding every byte it prints.
///
/// Homebrew echoes download URLs, cache paths and occasionally credentials from
/// the environment, none of which may reach a DTO, a toast or a log line. A
/// failure therefore reports only that it failed and where to look.
async fn homebrew_install(package: &str) -> Result<(), AppError> {
    let brew = find_program(PackageManager::Homebrew.program(), None)
        .ok_or_else(|| AppError::NotFound("Homebrew is not installed".into()))?;
    let mut command = Command::new(brew);
    command
        .arg("install")
        .arg(package)
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let status = tokio::time::timeout(INSTALL_TIMEOUT, command.status())
        .await
        .map_err(|_| AppError::Io("the install command timed out".into()))?
        .map_err(|err| AppError::Io(format!("could not start Homebrew: {err}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::Io(
            "the install did not complete; run the same command in a terminal to see why".into(),
        ))
    }
}

// ── Commands ──────────────────────────────────────────────────────────────

/// Report every prerequisite, or only one group's.
///
/// Each call spawns child processes (`gh auth status`, PATH walks), so the
/// caller is expected to cache aggressively rather than poll.
#[tauri::command]
pub async fn get_dependency_status(
    app_handle: AppHandle,
    group: Option<DependencyGroup>,
) -> Result<Vec<Dependency>, AppError> {
    dependencies_for(&app_handle, group).await
}

/// Perform the one step attached to a dependency, after the user asked for it.
///
/// Nothing here runs on its own: the command string reached the UI first, and
/// this is only ever reached from an explicit click. Steps the app must not take
/// on the user's behalf return `performed: false` with the instruction instead
/// of pretending to have run.
#[tauri::command]
pub async fn run_dependency_remediation(
    app_handle: AppHandle,
    dependency_id: String,
) -> Result<RemediationOutcome, AppError> {
    let current = dependency_by_id(&app_handle, &dependency_id).await?;
    let Some(remediation) = current.remediation.clone() else {
        return Ok(RemediationOutcome {
            dependency_id,
            performed: false,
            message: format!("{} is already set up.", current.label),
            dependency: current,
        });
    };

    let (performed, message) = match remediation.kind {
        RemediationKind::DocLink => {
            let url = remediation
                .url
                .clone()
                .ok_or_else(|| AppError::Invalid("this step has no documentation link".into()))?;
            tauri_plugin_opener::open_url(url, None::<&str>)
                .map_err(|err| AppError::Io(format!("open documentation: {err}")))?;
            (
                true,
                "Opened the install guide in your browser.".to_string(),
            )
        }
        RemediationKind::InAppAction => (false, remediation.label.clone()),
        RemediationKind::TerminalCommand if remediation.runnable => {
            let package = install_spec(&dependency_id)
                .and_then(|spec| spec.homebrew)
                .ok_or_else(|| {
                    AppError::Invalid("this step has no package Autostand can install".into())
                })?;
            homebrew_install(package).await?;
            (true, format!("{} is installed.", current.label))
        }
        RemediationKind::TerminalCommand => (
            false,
            remediation.command.clone().map_or_else(
                || remediation.label.clone(),
                |command| format!("Run `{command}` in your terminal."),
            ),
        ),
    };

    let dependency = dependency_by_id(&app_handle, &dependency_id).await?;
    Ok(RemediationOutcome {
        dependency_id,
        performed,
        message,
        dependency,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        dependency, detect_package_manager, find_program, group_of, in_app, includes,
        install_remediation, install_spec, local_ai_dependencies, model_dependency, probe_runtime,
        probe_sidecar, repo_sync_dependencies, Dependency, DependencyGroup, DependencyState,
        LocalProbe, PackageManager, RemediationKind, RemediationOutcome, RepoProbe, GH_AUTH_ID,
        GH_ID, GH_INSTALL, GIT_ID, MODEL_ID, RUNTIME_ID, RUNTIME_INSTALL, SIDECAR_ID,
    };
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    const EVERY_MANAGER: [PackageManager; 6] = [
        PackageManager::Homebrew,
        PackageManager::Winget,
        PackageManager::Apt,
        PackageManager::Dnf,
        PackageManager::Pacman,
        PackageManager::Zypper,
    ];

    fn by_id<'a>(list: &'a [Dependency], id: &str) -> &'a Dependency {
        list.iter()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("missing dependency {id}"))
    }

    #[cfg(unix)]
    fn executable(dir: &Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;

        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("fake program");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&path, permissions).expect("chmod");
        path
    }

    #[cfg(unix)]
    #[test]
    fn executable_detection_is_hermetic_and_does_not_use_the_ambient_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let git = executable(temp.path(), "git");
        assert_eq!(
            find_program("git", Some(temp.path().as_os_str())),
            Some(git)
        );
        assert_eq!(find_program("gh", Some(temp.path().as_os_str())), None);
        assert_eq!(find_program("gh", Some(OsStr::new(""))), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_package_manager_is_only_reported_when_it_exists_on_this_machine() {
        let temp = tempfile::tempdir().expect("tempdir");
        assert_eq!(detect_package_manager(Some(temp.path().as_os_str())), None);
        let expected = if cfg!(target_os = "macos") {
            PackageManager::Homebrew
        } else {
            PackageManager::Apt
        };
        executable(temp.path(), expected.program());
        assert_eq!(
            detect_package_manager(Some(temp.path().as_os_str())),
            Some(expected)
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_sidecar_override_is_ignored_unless_it_is_a_real_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let missing = temp.path().join("nowhere/autostand-local-llm");
        assert_eq!(
            probe_sidecar(Some(&missing.to_string_lossy()), Some(OsStr::new(""))),
            None
        );
        let real = executable(temp.path(), "autostand-local-llm");
        assert_eq!(
            probe_sidecar(Some(&real.to_string_lossy()), Some(OsStr::new(""))),
            Some(real)
        );
        assert_eq!(probe_sidecar(Some("   "), Some(OsStr::new(""))), None);
    }

    /// The sidecar prefers `llama-completion` and accepts `llama-cli`; a probe
    /// that only knew one of them would call a working install broken.
    #[cfg(unix)]
    #[test]
    fn the_runtime_probe_accepts_both_llama_cpp_program_names() {
        let temp = tempfile::tempdir().expect("tempdir");
        let legacy = temp.path().join("legacy");
        let modern = temp.path().join("modern");
        std::fs::create_dir_all(&legacy).expect("legacy dir");
        std::fs::create_dir_all(&modern).expect("modern dir");
        let cli = executable(&legacy, "llama-cli");
        let completion = executable(&modern, "llama-completion");

        assert_eq!(
            probe_runtime(None, None, Some(legacy.as_os_str())),
            Some(cli)
        );
        assert_eq!(
            probe_runtime(None, None, Some(modern.as_os_str())),
            Some(completion)
        );
        assert_eq!(probe_runtime(None, None, Some(OsStr::new(""))), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_runtime_beside_the_sidecar_wins_over_the_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let bundle = temp.path().join("bundle");
        let elsewhere = temp.path().join("elsewhere");
        std::fs::create_dir_all(&bundle).expect("bundle dir");
        std::fs::create_dir_all(&elsewhere).expect("elsewhere dir");
        let sidecar = executable(&bundle, "autostand-local-llm");
        let bundled = executable(&bundle, "llama-completion");
        executable(&elsewhere, "llama-cli");

        assert_eq!(
            probe_runtime(Some(&sidecar), None, Some(elsewhere.as_os_str())),
            Some(bundled)
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_explicit_runtime_override_wins_over_everything_else() {
        let temp = tempfile::tempdir().expect("tempdir");
        let override_path = executable(temp.path(), "my-llama");
        assert_eq!(
            probe_runtime(None, Some(override_path.as_os_str()), Some(OsStr::new(""))),
            Some(override_path)
        );
        assert_eq!(
            probe_runtime(
                None,
                Some(OsStr::new("/nowhere/llama")),
                Some(OsStr::new(""))
            ),
            None
        );
    }

    #[test]
    fn an_unmet_dependency_always_carries_an_actionable_next_step() {
        let mut all = repo_sync_dependencies(&RepoProbe::default(), None);
        all.extend(local_ai_dependencies(&LocalProbe::default(), None));

        assert_eq!(all.len(), 6);
        for item in &all {
            assert_ne!(item.state, DependencyState::Ok, "{}", item.id);
            let remediation = item
                .remediation
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no remediation", item.id));
            assert!(
                remediation.command.is_some()
                    || remediation.url.is_some()
                    || remediation.kind == RemediationKind::InAppAction,
                "{} offers nothing to act on",
                item.id
            );
        }
    }

    #[test]
    fn a_satisfied_dependency_offers_no_remediation() {
        let probe = RepoProbe {
            git: Some(PathBuf::from("/usr/bin/git")),
            gh: Some(PathBuf::from("/usr/bin/gh")),
            authenticated: Some(true),
        };
        let repo = repo_sync_dependencies(&probe, Some(PackageManager::Homebrew));
        for item in &repo {
            assert_eq!(item.state, DependencyState::Ok, "{}", item.id);
            assert!(item.remediation.is_none(), "{}", item.id);
        }
        assert_eq!(by_id(&repo, GIT_ID).detail.as_deref(), Some("/usr/bin/git"));
    }

    /// "Signed out" and "cannot be checked" are different answers, and only one
    /// of them is the user's to fix right now.
    #[test]
    fn sign_in_is_unknown_rather_than_missing_without_the_cli() {
        let without_cli = repo_sync_dependencies(&RepoProbe::default(), None);
        assert_eq!(
            by_id(&without_cli, GH_AUTH_ID).state,
            DependencyState::Unknown
        );

        let signed_out = RepoProbe {
            git: Some(PathBuf::from("/usr/bin/git")),
            gh: Some(PathBuf::from("/usr/bin/gh")),
            authenticated: Some(false),
        };
        let reported = repo_sync_dependencies(&signed_out, None);
        assert_eq!(
            by_id(&reported, GH_AUTH_ID).state,
            DependencyState::Misconfigured
        );
        assert_eq!(by_id(&reported, GH_ID).state, DependencyState::Ok);
    }

    /// Signing in opens a browser and waits for a code; running it headless
    /// would hang forever behind a spinner.
    #[test]
    fn signing_in_is_shown_but_never_run_by_autostand() {
        let reported = repo_sync_dependencies(&RepoProbe::default(), None);
        let remediation = by_id(&reported, GH_AUTH_ID)
            .remediation
            .as_ref()
            .expect("sign-in remediation");
        assert_eq!(remediation.kind, RemediationKind::TerminalCommand);
        assert!(!remediation.runnable);
        assert_eq!(
            remediation.command.as_deref(),
            Some("gh auth login --hostname github.com --web")
        );
    }

    #[test]
    fn homebrew_is_the_only_manager_autostand_runs_itself() {
        for manager in EVERY_MANAGER {
            let (command, runnable) = manager.install_command("git");
            assert!(command.contains("git"), "{command}");
            assert_eq!(
                runnable,
                manager == PackageManager::Homebrew,
                "{manager:?} runnability"
            );
        }
    }

    /// A command that would fail is worse than a link that works. llama.cpp has
    /// no package id this project can vouch for outside Homebrew.
    #[test]
    fn a_manager_without_a_verified_package_falls_back_to_the_official_guide() {
        for manager in EVERY_MANAGER {
            let remediation = install_remediation(&RUNTIME_INSTALL, Some(manager));
            if manager == PackageManager::Homebrew {
                assert_eq!(remediation.kind, RemediationKind::TerminalCommand);
                assert_eq!(
                    remediation.command.as_deref(),
                    Some("brew install llama.cpp")
                );
            } else {
                assert_eq!(remediation.kind, RemediationKind::DocLink, "{manager:?}");
                assert!(remediation.command.is_none(), "{manager:?}");
                assert_eq!(
                    remediation.url.as_deref(),
                    Some(RUNTIME_INSTALL.doc_url),
                    "{manager:?}"
                );
            }
        }
        // apt and zypper do not carry `gh` in their default repositories.
        for manager in [PackageManager::Apt, PackageManager::Zypper] {
            assert_eq!(
                install_remediation(&GH_INSTALL, Some(manager)).kind,
                RemediationKind::DocLink
            );
        }
    }

    #[test]
    fn no_package_manager_still_yields_a_documentation_link() {
        let remediation = install_remediation(&GH_INSTALL, None);
        assert_eq!(remediation.kind, RemediationKind::DocLink);
        assert!(!remediation.runnable);
        assert_eq!(remediation.url.as_deref(), Some(GH_INSTALL.doc_url));
    }

    #[test]
    fn only_installable_dependencies_expose_a_homebrew_package() {
        assert_eq!(
            install_spec(GIT_ID).and_then(|spec| spec.homebrew),
            Some("git")
        );
        assert_eq!(
            install_spec(RUNTIME_ID).and_then(|spec| spec.homebrew),
            Some("llama.cpp")
        );
        assert!(install_spec(GH_AUTH_ID).is_none());
        assert!(install_spec(SIDECAR_ID).is_none());
        assert!(install_spec(MODEL_ID).is_none());
    }

    #[test]
    fn the_model_is_missing_selected_or_downloaded_but_unused() {
        let nothing = model_dependency(&LocalProbe::default());
        assert_eq!(nothing.state, DependencyState::Missing);

        let downloaded = model_dependency(&LocalProbe {
            installed: vec!["qwen3.5:2b".to_string()],
            ..LocalProbe::default()
        });
        assert_eq!(downloaded.state, DependencyState::Misconfigured);

        let ready = model_dependency(&LocalProbe {
            selected: Some("qwen3.5:2b".to_string()),
            installed: vec!["qwen3.5:2b".to_string()],
            ..LocalProbe::default()
        });
        assert_eq!(ready.state, DependencyState::Ok);
        assert!(ready.remediation.is_none());

        // A selection whose file was deleted is stale, not satisfied.
        let stale = model_dependency(&LocalProbe {
            selected: Some("gemma3:4b".to_string()),
            installed: vec!["qwen3.5:2b".to_string()],
            ..LocalProbe::default()
        });
        assert_eq!(stale.state, DependencyState::Misconfigured);
    }

    /// A missing runtime must not make the catalog look unusable: downloading a
    /// GGUF stays possible and is a prerequisite for using one later.
    #[test]
    fn a_missing_runtime_does_not_invalidate_a_downloaded_model() {
        let reported = local_ai_dependencies(
            &LocalProbe {
                sidecar: Some(PathBuf::from("/apps/autostand-local-llm")),
                runtime: None,
                selected: Some("qwen3.5:2b".to_string()),
                installed: vec!["qwen3.5:2b".to_string()],
            },
            None,
        );
        assert_eq!(by_id(&reported, RUNTIME_ID).state, DependencyState::Missing);
        assert_eq!(by_id(&reported, MODEL_ID).state, DependencyState::Ok);
        assert_eq!(by_id(&reported, SIDECAR_ID).state, DependencyState::Ok);
    }

    #[test]
    fn the_runtime_step_mentions_the_override_every_platform_shares() {
        let remediation = by_id(
            &local_ai_dependencies(&LocalProbe::default(), None),
            RUNTIME_ID,
        )
        .remediation
        .clone()
        .expect("runtime remediation");
        assert!(remediation
            .note
            .unwrap_or_default()
            .contains("AUTOSTAND_LLAMA_CLI"));
    }

    /// Every reported id must route back to its own group, or remediating one
    /// dependency would re-probe the other feature — and an id nobody owns must
    /// not silently resolve to a group.
    #[test]
    fn every_reported_id_routes_back_to_its_own_group() {
        let mut all = repo_sync_dependencies(&RepoProbe::default(), None);
        all.extend(local_ai_dependencies(&LocalProbe::default(), None));
        for item in &all {
            assert_eq!(group_of(&item.id), Some(item.group), "{}", item.id);
        }
        assert_eq!(group_of("repo-sync.svn"), None);
    }

    #[test]
    fn a_group_filter_selects_exactly_that_group() {
        assert!(includes(None, DependencyGroup::RepoSync));
        assert!(includes(None, DependencyGroup::LocalAi));
        assert!(includes(
            Some(DependencyGroup::RepoSync),
            DependencyGroup::RepoSync
        ));
        assert!(!includes(
            Some(DependencyGroup::RepoSync),
            DependencyGroup::LocalAi
        ));
        assert!(!includes(
            Some(DependencyGroup::LocalAi),
            DependencyGroup::RepoSync
        ));
    }

    #[test]
    fn the_wire_format_matches_the_documented_contract() {
        let value = serde_json::to_value(&repo_sync_dependencies(&RepoProbe::default(), None)[0])
            .expect("dependency json");
        let object = value.as_object().expect("object");
        let mut keys = object.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        assert_eq!(
            keys,
            [
                "description",
                "detail",
                "group",
                "id",
                "label",
                "remediation",
                "state"
            ]
        );
        assert_eq!(object["group"], serde_json::json!("repo_sync"));
        assert_eq!(object["state"], serde_json::json!("missing"));
        assert_eq!(object["remediation"]["kind"], serde_json::json!("doc_link"));

        let with_manager = serde_json::to_value(
            &repo_sync_dependencies(&RepoProbe::default(), Some(PackageManager::Homebrew))[0],
        )
        .expect("dependency json");
        assert_eq!(
            with_manager["remediation"]["kind"],
            serde_json::json!("terminal_command")
        );
        assert_eq!(
            with_manager["remediation"]["command"],
            serde_json::json!("brew install git")
        );
        assert_eq!(
            with_manager["remediation"]["runnable"],
            serde_json::json!(true)
        );
    }

    /// `gh auth status` prints token diagnostics and `brew` prints paths and
    /// URLs; neither may ever ride out on a DTO.
    #[test]
    fn no_dto_field_can_carry_command_output() {
        let outcome = RemediationOutcome {
            dependency_id: GIT_ID.to_string(),
            performed: true,
            message: "Git is installed.".to_string(),
            dependency: dependency(
                GIT_ID,
                DependencyGroup::RepoSync,
                "Git",
                "Commits and pushes the standup history from the sync folder.",
                DependencyState::Missing,
                None,
                Some(in_app("Install Git.")),
            ),
        };
        let json = serde_json::to_string(&outcome).expect("outcome json");
        assert!(!json.contains("stdout"));
        assert!(!json.contains("stderr"));
        let value = serde_json::to_value(&outcome).expect("outcome value");
        let mut keys = value
            .as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(
            keys,
            ["dependency", "dependency_id", "message", "performed"]
        );
    }
}
