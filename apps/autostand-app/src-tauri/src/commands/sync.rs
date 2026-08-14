//! Cloud-folder and GitHub repository sync commands.
//!
//! Cloud Sync and Repo Sync share one working directory but remain independent
//! transports. A selected cloud root always gets an `autostand/` child; the
//! pipeline writes `<date>.md` files there. Repo Sync may then initialise that
//! exact child as a private GitHub repository, allowing both transports to
//! coexist without copying, moving, or deleting the user's files.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::process::Command;

use crate::commands::dependencies::find_program;
use crate::commands::{load_config, save_config};
use crate::error::AppError;

const CLOUD_CHILD: &str = "autostand";
const DEFAULT_REPO_NAME: &str = "autostand-dailies";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const SETUP_TIMEOUT: Duration = Duration::from_secs(180);

/// One detected cloud root. `dailies_path` is the directory Autostand uses;
/// `path` remains the provider root so the UI can explain the layout clearly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudFolder {
    pub id: String,
    pub label: String,
    pub path: String,
    pub dailies_path: String,
    pub exists: bool,
    pub provider: String,
}

/// Result of selecting a cloud root and preparing its `autostand/` child.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudSyncSelection {
    pub root_path: String,
    pub dailies_path: String,
    pub created: bool,
}

/// Effective Repo Sync state. `configured` is the persisted user preference;
/// `enabled` additionally requires both CLIs, a valid GitHub login, a git
/// working tree, an origin, and a repository verified as private.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct RepoSyncStatus {
    pub git_available: bool,
    pub gh_available: bool,
    pub gh_authenticated: bool,
    pub can_setup: bool,
    pub configured: bool,
    pub enabled: bool,
    pub repo_path: String,
    pub repository: Option<String>,
    pub private: Option<bool>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhRepoView {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
    #[serde(rename = "isPrivate")]
    is_private: bool,
}

#[derive(Debug)]
struct Output {
    success: bool,
    stdout: String,
}

/// Resolved executables rather than shell command strings. Detection itself
/// lives in [`crate::commands::dependencies`], so the Repo Sync card and the
/// requirement checklist can never disagree about what is installed.
#[derive(Debug, Clone)]
struct RepoTools {
    git: Option<PathBuf>,
    gh: Option<PathBuf>,
}

impl RepoTools {
    fn detect(path_override: Option<&OsStr>) -> Self {
        Self {
            git: find_program("git", path_override),
            gh: find_program("gh", path_override),
        }
    }

    async fn run(
        &self,
        program: &Path,
        args: &[&OsStr],
        cwd: Option<&Path>,
        timeout: Duration,
    ) -> Result<Output, AppError> {
        let mut command = Command::new(program);
        command
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GH_PROMPT_DISABLED", "1")
            .kill_on_drop(true);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        let result = tokio::time::timeout(timeout, command.output())
            .await
            .map_err(|_| AppError::Git("repository command timed out".into()))?
            .map_err(|err| AppError::Git(format!("could not start repository command: {err}")))?;
        Ok(Output {
            success: result.status.success(),
            stdout: String::from_utf8_lossy(&result.stdout).trim().to_string(),
        })
    }

    async fn git(&self, dir: &Path, args: &[&str]) -> Result<Output, AppError> {
        let program = self
            .git
            .as_deref()
            .ok_or_else(|| AppError::Git("git is not installed".into()))?;
        let args = args.iter().map(OsStr::new).collect::<Vec<_>>();
        self.run(program, &args, Some(dir), COMMAND_TIMEOUT).await
    }

    async fn gh(&self, dir: Option<&Path>, args: &[&str]) -> Result<Output, AppError> {
        let program = self
            .gh
            .as_deref()
            .ok_or_else(|| AppError::Git("GitHub CLI is not installed".into()))?;
        let args = args.iter().map(OsStr::new).collect::<Vec<_>>();
        self.run(program, &args, dir, SETUP_TIMEOUT).await
    }

    async fn gh_with_path_arg(&self, dir: &Path, repo_name: &str) -> Result<Output, AppError> {
        let program = self
            .gh
            .as_deref()
            .ok_or_else(|| AppError::Git("GitHub CLI is not installed".into()))?;
        let source = dir.as_os_str();
        let args = [
            OsStr::new("repo"),
            OsStr::new("create"),
            OsStr::new(repo_name),
            OsStr::new("--private"),
            OsStr::new("--source"),
            source,
            OsStr::new("--remote"),
            OsStr::new("origin"),
        ];
        self.run(program, &args, Some(dir), SETUP_TIMEOUT).await
    }
}

fn as_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn dailies_path(root: &Path) -> PathBuf {
    root.join(CLOUD_CHILD)
}

fn folder(id: &str, label: &str, provider: &str, path: &Path) -> CloudFolder {
    CloudFolder {
        id: id.to_string(),
        label: label.to_string(),
        dailies_path: as_string(&dailies_path(path)),
        exists: path.is_dir(),
        path: as_string(path),
        provider: provider.to_string(),
    }
}

fn slug(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    normalized
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn push_unique(out: &mut Vec<CloudFolder>, candidate: CloudFolder) {
    if !out.iter().any(|item| item.path == candidate.path) {
        out.push(candidate);
    }
}

fn macos_cloud_folders(home: &Path) -> Vec<CloudFolder> {
    let mut out = vec![
        folder(
            "icloud-drive",
            "iCloud Drive",
            "iCloud",
            &home.join("Library/Mobile Documents/com~apple~CloudDocs"),
        ),
        folder(
            "icloud-private",
            "iCloud (Autostand private)",
            "iCloud",
            &home.join("Library/Mobile Documents/com.miguel50flowers.autostand"),
        ),
    ];
    let base = home.join("Library/CloudStorage");
    if let Ok(entries) = std::fs::read_dir(base) {
        let mut entries = entries.flatten().collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            let lower = name.to_ascii_lowercase();
            let descriptor = if lower.starts_with("onedrive") {
                Some(("onedrive", "OneDrive", "OneDrive"))
            } else if lower.starts_with("google") || lower.contains("google-drive") {
                Some(("google-drive", "Google Drive", "Google Drive"))
            } else {
                None
            };
            if let Some((id, label, provider)) = descriptor {
                push_unique(
                    &mut out,
                    folder(
                        &format!("{id}-{}", slug(&name)),
                        &format!("{label} — {name}"),
                        provider,
                        &entry.path(),
                    ),
                );
            }
        }
    }
    out
}

fn windows_cloud_folders(home: &Path) -> Vec<CloudFolder> {
    let mut out = Vec::new();
    for (id, label, provider, path) in [
        (
            "icloud-drive",
            "iCloud Drive",
            "iCloud",
            home.join("iCloudDrive"),
        ),
        ("onedrive", "OneDrive", "OneDrive", home.join("OneDrive")),
        (
            "google-drive",
            "Google Drive",
            "Google Drive",
            home.join("Google Drive"),
        ),
    ] {
        push_unique(&mut out, folder(id, label, provider, &path));
    }
    for key in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(path) = std::env::var_os(key).map(PathBuf::from) {
            push_unique(
                &mut out,
                folder(
                    &format!("onedrive-{}", slug(key)),
                    &format!("OneDrive ({key})"),
                    "OneDrive",
                    &path,
                ),
            );
        }
    }
    out
}

fn linux_cloud_folders(home: &Path) -> Vec<CloudFolder> {
    vec![
        folder("onedrive", "OneDrive", "OneDrive", &home.join("OneDrive")),
        folder(
            "google-drive",
            "Google Drive",
            "Google Drive",
            &home.join("Google Drive"),
        ),
    ]
}

fn detected_cloud_folders(home: Option<&Path>) -> Vec<CloudFolder> {
    let Some(home) = home else {
        return Vec::new();
    };
    if cfg!(target_os = "macos") {
        macos_cloud_folders(home)
    } else if cfg!(target_os = "windows") {
        windows_cloud_folders(home)
    } else {
        linux_cloud_folders(home)
    }
}

fn prepare_cloud_root(root: &Path) -> Result<CloudSyncSelection, AppError> {
    if !root.is_absolute() {
        return Err(AppError::Invalid(
            "cloud sync root must be an absolute path".into(),
        ));
    }
    if !root.is_dir() {
        return Err(AppError::NotFound(
            "cloud sync root is not an existing directory".into(),
        ));
    }
    let target = dailies_path(root);
    let created = !target.exists();
    if target.exists() && !target.is_dir() {
        return Err(AppError::Invalid(
            "the cloud root already contains a non-directory named autostand".into(),
        ));
    }
    std::fs::create_dir_all(&target)?;
    Ok(CloudSyncSelection {
        root_path: as_string(root),
        dailies_path: as_string(&target),
        created,
    })
}

fn valid_repo_name(value: &str) -> bool {
    let len = value.len();
    (1..=100).contains(&len)
        && value != "."
        && value != ".."
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'.' | b'_' | b'-'))
}

async fn gh_authenticated(tools: &RepoTools) -> bool {
    match tools.gh.as_deref() {
        Some(gh) => crate::commands::dependencies::gh_authenticated(gh).await,
        None => false,
    }
}

async fn origin_exists(tools: &RepoTools, dir: &Path) -> bool {
    tools
        .git(dir, &["remote", "get-url", "origin"])
        .await
        .is_ok_and(|output| output.success && !output.stdout.is_empty())
}

async fn repo_view(tools: &RepoTools, dir: &Path) -> Option<GhRepoView> {
    let output = tools
        .gh(
            Some(dir),
            &["repo", "view", "--json", "nameWithOwner,isPrivate"],
        )
        .await
        .ok()?;
    if !output.success {
        return None;
    }
    serde_json::from_str(&output.stdout).ok()
}

async fn repo_status_for(config: &super::types::AppConfig, tools: &RepoTools) -> RepoSyncStatus {
    let dir = super::standup::resolve_dailies_dir(
        Some(config.dailies_dir.as_str()),
        dirs::home_dir().as_deref(),
    );
    let git_available = tools.git.is_some();
    let gh_available = tools.gh.is_some();
    let gh_authenticated = gh_available && gh_authenticated(tools).await;
    let is_repo = crate::git_ops::is_git_repo(&dir);
    let has_origin = git_available && is_repo && origin_exists(tools, &dir).await;
    let view = if gh_authenticated && has_origin {
        repo_view(tools, &dir).await
    } else {
        None
    };
    let private = view.as_ref().map(|repo| repo.is_private);
    let enabled = config.sync.repo_enabled
        && git_available
        && gh_available
        && gh_authenticated
        && is_repo
        && has_origin
        && private == Some(true);
    // The requirement checklist (`commands::dependencies`) names the missing
    // tool and carries the command that installs it, so this message only has
    // to say what it costs Repo Sync — repeating the fix here would give the
    // user two half-instructions instead of one whole one.
    let message = if !git_available || !gh_available || !gh_authenticated {
        Some("Repo Sync stays off until its requirements are met.".into())
    } else if config.sync.repo_enabled && !is_repo {
        Some("Repo Sync needs to be set up for this dailies folder.".into())
    } else if has_origin && private == Some(false) {
        Some(
            "The configured GitHub repository is public; Repo Sync requires a private repository."
                .into(),
        )
    } else if has_origin && private.is_none() {
        Some("The origin could not be verified as a private GitHub repository.".into())
    } else if config.sync.repo_enabled && !has_origin {
        Some("Repo Sync has no origin remote.".into())
    } else {
        None
    };
    RepoSyncStatus {
        git_available,
        gh_available,
        gh_authenticated,
        can_setup: git_available && gh_available && gh_authenticated,
        configured: config.sync.repo_enabled,
        enabled,
        repo_path: as_string(&dir),
        repository: view.map(|repo| repo.name_with_owner),
        private,
        message,
    }
}

async fn ensure_local_identity(tools: &RepoTools, dir: &Path) -> Result<(), AppError> {
    let has_name = tools
        .git(dir, &["config", "--get", "user.name"])
        .await?
        .success;
    let has_email = tools
        .git(dir, &["config", "--get", "user.email"])
        .await?
        .success;
    if has_name && has_email {
        return Ok(());
    }

    let login = tools.gh(None, &["api", "user", "--jq", ".login"]).await?;
    let id = tools.gh(None, &["api", "user", "--jq", ".id"]).await?;
    let safe_login = !login.stdout.is_empty()
        && login
            .stdout
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-');
    let safe_id = !id.stdout.is_empty() && id.stdout.bytes().all(|ch| ch.is_ascii_digit());
    if !login.success || !id.success || !safe_login || !safe_id {
        return Err(AppError::Git(
            "Git identity is missing and could not be derived from GitHub".into(),
        ));
    }
    if !has_name
        && !tools
            .git(dir, &["config", "user.name", &login.stdout])
            .await?
            .success
    {
        return Err(AppError::Git("could not configure local Git name".into()));
    }
    if !has_email {
        let email = format!("{}+{}@users.noreply.github.com", id.stdout, login.stdout);
        if !tools
            .git(dir, &["config", "user.email", &email])
            .await?
            .success
        {
            return Err(AppError::Git("could not configure local Git email".into()));
        }
    }
    Ok(())
}

async fn initialize_repo(tools: &RepoTools, dir: &Path) -> Result<(), AppError> {
    if crate::git_ops::is_git_repo(dir) {
        return Ok(());
    }
    let init = tools.git(dir, &["init", "--quiet"]).await?;
    if !init.success {
        return Err(AppError::Git(
            "could not initialize the local repository".into(),
        ));
    }
    let branch = tools
        .git(dir, &["symbolic-ref", "HEAD", "refs/heads/main"])
        .await?;
    if !branch.success {
        return Err(AppError::Git("could not select the main branch".into()));
    }
    Ok(())
}

async fn initial_attributes_commit(tools: &RepoTools, dir: &Path) -> Result<(), AppError> {
    let has_head = tools
        .git(dir, &["rev-parse", "--verify", "HEAD"])
        .await?
        .success;
    if has_head {
        return Ok(());
    }
    let add = tools.git(dir, &["add", "--", ".gitattributes"]).await?;
    if !add.success {
        return Err(AppError::Git(
            "could not stage repository safeguards".into(),
        ));
    }
    let commit = tools
        .git(
            dir,
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--message",
                "chore: configure autostand repository",
                "--",
                ".gitattributes",
            ],
        )
        .await?;
    if !commit.success {
        return Err(AppError::Git(
            "could not create the repository setup commit".into(),
        ));
    }
    Ok(())
}

/// Cheap headless gate used before the pipeline invokes Git. It deliberately
/// discards all command output so an authentication failure can never leak a
/// credential-bearing diagnostic into application logs.
pub(crate) async fn repo_sync_prerequisites() -> bool {
    let tools = RepoTools::detect(None);
    tools.git.is_some() && tools.gh.is_some() && gh_authenticated(&tools).await
}

/// Keep the pipeline's transport gate explicit and testable. A persisted
/// preference alone can never enable Git when the runtime prerequisites fail.
pub(crate) const fn should_run_repo_sync(
    config: &super::types::AppConfig,
    prerequisites_available: bool,
) -> bool {
    config.sync.repo_enabled && prerequisites_available
}

#[tauri::command]
pub async fn detect_cloud_folders() -> Result<Vec<CloudFolder>, AppError> {
    Ok(detected_cloud_folders(dirs::home_dir().as_deref()))
}

/// Prepare `<root>/autostand` and atomically switch future runs to it. Existing
/// standup files elsewhere are deliberately left where they are.
#[tauri::command]
pub async fn configure_cloud_sync(
    app_handle: AppHandle,
    root_path: String,
) -> Result<CloudSyncSelection, AppError> {
    let selection = prepare_cloud_root(Path::new(root_path.trim()))?;
    let mut config = load_config(&app_handle)?;
    if config.dailies_dir != selection.dailies_path {
        // Repo Sync is bound to a working tree, not to the provider account.
        // A new cloud root must be explicitly set up/verified before Git may
        // touch it; the old repository and its files remain untouched.
        config.sync.repo_enabled = false;
    }
    config.dailies_dir.clone_from(&selection.dailies_path);
    config.sync.cloud_root = Some(selection.root_path.clone());
    save_config(&app_handle, &config)?;
    Ok(selection)
}

#[tauri::command]
pub async fn get_repo_sync_status(app_handle: AppHandle) -> Result<RepoSyncStatus, AppError> {
    let config = load_config(&app_handle)?;
    Ok(repo_status_for(&config, &RepoTools::detect(None)).await)
}

/// Create (or validate) a private GitHub repository for the current dailies
/// directory. No existing file is staged except the generated `.gitattributes`
/// in a brand-new repository, and no path is moved or removed.
#[tauri::command]
pub async fn setup_repo_sync(
    app_handle: AppHandle,
    repo_name: Option<String>,
) -> Result<RepoSyncStatus, AppError> {
    let mut config = load_config(&app_handle)?;
    let tools = RepoTools::detect(None);
    if tools.git.is_none() || tools.gh.is_none() || !gh_authenticated(&tools).await {
        return Err(AppError::Git(
            "Repo Sync requires Git, GitHub CLI, and an authenticated GitHub account".into(),
        ));
    }
    let name = repo_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_REPO_NAME);
    if !valid_repo_name(name) {
        return Err(AppError::Invalid(
            "repository name may contain only letters, numbers, '.', '_' and '-'".into(),
        ));
    }

    let dir = super::standup::resolve_dailies_dir(
        Some(config.dailies_dir.as_str()),
        dirs::home_dir().as_deref(),
    );
    std::fs::create_dir_all(&dir)?;
    initialize_repo(&tools, &dir).await?;
    let existing_origin = origin_exists(&tools, &dir).await;
    if existing_origin {
        let existing = repo_view(&tools, &dir).await.ok_or_else(|| {
            AppError::Git("the existing origin is not a verifiable GitHub repository".into())
        })?;
        if !existing.is_private {
            return Err(AppError::Invalid(
                "the existing origin is public; Repo Sync requires a private repository".into(),
            ));
        }
    }
    crate::git_ops::ensure_gitattributes(&dir)
        .map_err(|err| AppError::Git(format!("could not configure safe merges: {err}")))?;
    ensure_local_identity(&tools, &dir).await?;
    initial_attributes_commit(&tools, &dir).await?;

    if !existing_origin {
        let created = tools.gh_with_path_arg(&dir, name).await?;
        if !created.success {
            return Err(AppError::Git(
                "GitHub CLI could not create the private repository".into(),
            ));
        }
    }

    // A failed initial push is recoverable: the regular pipeline retries it.
    // Never include stderr here; Git helpers may receive credential diagnostics.
    let _push = tools
        .git(&dir, &["push", "--set-upstream", "origin", "HEAD"])
        .await?;
    config.sync.repo_enabled = true;
    save_config(&app_handle, &config)?;
    Ok(repo_status_for(&config, &tools).await)
}

#[cfg(test)]
mod tests {
    use super::{
        dailies_path, detected_cloud_folders, macos_cloud_folders, prepare_cloud_root,
        repo_status_for, should_run_repo_sync, valid_repo_name, CloudFolder, RepoSyncStatus,
        RepoTools, CLOUD_CHILD,
    };
    use crate::commands::types::{AppConfig, SyncConfig};
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    #[test]
    fn every_cloud_root_maps_to_an_autostand_child() {
        let root = Path::new("/cloud/provider");
        assert_eq!(dailies_path(root), root.join(CLOUD_CHILD));
    }

    #[test]
    fn preparing_a_cloud_root_is_idempotent_and_preserves_existing_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("2026-08-14.md"), "legacy").expect("legacy file");
        let first = prepare_cloud_root(temp.path()).expect("prepare");
        assert!(first.created);
        assert_eq!(
            PathBuf::from(&first.dailies_path),
            temp.path().join("autostand")
        );
        assert_eq!(
            std::fs::read_to_string(temp.path().join("2026-08-14.md")).expect("legacy remains"),
            "legacy"
        );
        let second = prepare_cloud_root(temp.path()).expect("prepare again");
        assert!(!second.created);
    }

    #[test]
    fn preparing_a_cloud_root_rejects_relative_or_missing_paths() {
        assert!(prepare_cloud_root(Path::new("relative/cloud")).is_err());
        let temp = tempfile::tempdir().expect("tempdir");
        assert!(prepare_cloud_root(&temp.path().join("missing")).is_err());
    }

    #[test]
    fn cloud_child_must_not_replace_a_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("autostand"), "occupied").expect("file");
        assert!(prepare_cloud_root(temp.path()).is_err());
    }

    #[test]
    fn macos_contract_includes_icloud_private_google_and_onedrive_layouts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let storage = temp.path().join("Library/CloudStorage");
        std::fs::create_dir_all(storage.join("GoogleDrive-personal")).expect("google");
        std::fs::create_dir_all(storage.join("OneDrive-Work")).expect("onedrive");
        let folders = macos_cloud_folders(temp.path());
        assert!(folders.iter().any(|item| item.id == "icloud-drive"));
        assert!(folders.iter().any(|item| item.id == "icloud-private"));
        assert!(folders.iter().any(|item| item.provider == "Google Drive"));
        assert!(folders.iter().any(|item| item.provider == "OneDrive"));
        assert!(folders
            .iter()
            .all(|item| item.dailies_path == format!("{}/autostand", item.path)));
    }

    #[test]
    fn no_home_means_no_fabricated_cloud_paths() {
        assert!(detected_cloud_folders(None).is_empty());
    }

    #[test]
    fn repository_names_cannot_be_options_paths_or_owner_qualified() {
        for valid in ["autostand-dailies", "standups_2026", "daily.notes"] {
            assert!(valid_repo_name(valid), "{valid}");
        }
        for invalid in ["", "../repo", "owner/repo", "--public", "with space"] {
            assert!(!valid_repo_name(invalid), "{invalid}");
        }
    }

    #[test]
    fn pipeline_gate_requires_both_preference_and_runtime_prerequisites() {
        let mut config = AppConfig::default();
        assert!(!should_run_repo_sync(&config, false));
        assert!(!should_run_repo_sync(&config, true));
        config.sync.repo_enabled = true;
        assert!(!should_run_repo_sync(&config, false));
        assert!(should_run_repo_sync(&config, true));
    }

    #[test]
    fn existing_configs_without_sync_fields_upgrade_safely_disabled() {
        let mut value = serde_json::to_value(AppConfig::default()).expect("config json");
        value.as_object_mut().expect("config object").remove("sync");
        let config: AppConfig = serde_json::from_value(value).expect("legacy config");
        assert_eq!(config.sync, SyncConfig::default());
        assert!(!config.sync.repo_enabled);
    }

    /// `RepoTools` resolves through the shared probe, so a hermetic PATH must
    /// reach it: the ambient developer PATH would make this pass anywhere.
    #[cfg(unix)]
    #[test]
    fn repo_tools_resolve_through_the_shared_hermetic_probe() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::tempdir().expect("tempdir");
        let git = temp.path().join("git");
        std::fs::write(&git, "#!/bin/sh\nexit 0\n").expect("fake git");
        let mut permissions = std::fs::metadata(&git).expect("metadata").permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&git, permissions).expect("chmod");

        let tools = RepoTools::detect(Some(temp.path().as_os_str()));
        assert_eq!(tools.git.as_deref(), Some(git.as_path()));
        assert!(tools.gh.is_none());
        assert!(RepoTools::detect(Some(OsStr::new(""))).git.is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn effective_repo_status_requires_authenticated_private_repo() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::tempdir().expect("tempdir");
        let bin = temp.path().join("bin");
        let repo = temp.path().join("cloud/autostand");
        std::fs::create_dir_all(repo.join(".git")).expect("repo layout");
        std::fs::create_dir_all(&bin).expect("bin");
        let git = bin.join("git");
        let gh = bin.join("gh");
        std::fs::write(
            &git,
            "#!/bin/sh\nif [ \"$1\" = remote ]; then echo git@github.com:owner/dailies.git; fi\nexit 0\n",
        )
        .expect("fake git");
        std::fs::write(
            &gh,
            "#!/bin/sh\nif [ \"$1\" = repo ]; then echo '{\"nameWithOwner\":\"owner/dailies\",\"isPrivate\":true}'; fi\nexit 0\n",
        )
        .expect("fake gh");
        for executable in [&git, &gh] {
            let mut permissions = std::fs::metadata(executable)
                .expect("metadata")
                .permissions();
            permissions.set_mode(0o700);
            std::fs::set_permissions(executable, permissions).expect("chmod");
        }

        let tools = RepoTools::detect(Some(bin.as_os_str()));
        let config = AppConfig {
            dailies_dir: repo.to_string_lossy().to_string(),
            sync: SyncConfig {
                cloud_root: Some(temp.path().join("cloud").to_string_lossy().to_string()),
                repo_enabled: true,
            },
            ..AppConfig::default()
        };
        let status = repo_status_for(&config, &tools).await;
        assert!(status.can_setup);
        assert!(status.configured);
        assert!(status.enabled);
        assert_eq!(status.repository.as_deref(), Some("owner/dailies"));
        assert_eq!(status.private, Some(true));
        assert_eq!(status.repo_path, repo.to_string_lossy());
    }

    /// The requirement checklist names the missing tool and carries the command
    /// that installs it. Repeating a second, weaker instruction here would give
    /// the user two half-answers.
    #[tokio::test]
    async fn a_missing_tool_defers_the_fix_to_the_requirement_checklist() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = AppConfig {
            dailies_dir: temp.path().to_string_lossy().to_string(),
            ..AppConfig::default()
        };
        let status = repo_status_for(&config, &RepoTools::detect(Some(OsStr::new("")))).await;

        assert!(!status.can_setup);
        let message = status.message.expect("a reason Repo Sync is off");
        assert_eq!(
            message,
            "Repo Sync stays off until its requirements are met."
        );
        assert!(!message.contains("gh auth login"));
        assert!(!message.contains("Install"));
    }

    #[test]
    fn sync_dtos_do_not_expose_a_remote_url_or_command_output() {
        let folder = CloudFolder {
            id: "icloud-drive".into(),
            label: "iCloud Drive".into(),
            path: "/cloud".into(),
            dailies_path: "/cloud/autostand".into(),
            exists: true,
            provider: "iCloud".into(),
        };
        let status = RepoSyncStatus {
            git_available: true,
            gh_available: true,
            gh_authenticated: true,
            can_setup: true,
            configured: true,
            enabled: true,
            repo_path: "/cloud/autostand".into(),
            repository: Some("owner/private-repo".into()),
            private: Some(true),
            message: None,
        };
        let folder_json = serde_json::to_value(folder).expect("folder json");
        let status_json = serde_json::to_value(status).expect("status json");
        assert!(folder_json.get("dailies_path").is_some());
        assert!(status_json.get("remote").is_none());
        assert!(status_json.get("stderr").is_none());
    }
}
