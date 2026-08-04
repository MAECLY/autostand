//! Repo discovery IPC command.
//!
//! See `docs/tauri/02-ipc-contracts.md` row `discover_repos`.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::commands::{load_config, resolve_dir, types::RepoInfo};
use crate::error::AppError;

/// Fallback location for `github_dir` under the user's home.
const GITHUB_DIR_FALLBACK: &[&str] = &["Documents", "Github"];

/// Resolve the repo-scan root: the configured `github_dir`, else
/// `<home>/Documents/Github`.
pub(crate) fn resolve_github_dir(configured: Option<&str>, home: Option<&Path>) -> PathBuf {
    resolve_dir(configured, home, GITHUB_DIR_FALLBACK)
}

/// Resolve the `github_dir` from the persisted config.
fn github_dir(app: &impl tauri::Manager<tauri::Wry>) -> PathBuf {
    let config = load_config(app).ok();
    resolve_github_dir(
        config.as_ref().map(|c| c.github_dir.as_str()),
        dirs::home_dir().as_deref(),
    )
}

/// Run `git config --get remote.origin.url` in `dir`.
async fn git_remote(dir: &std::path::Path) -> Option<String> {
    let out = tokio::process::Command::new("git")
        .arg("config")
        .arg("--get")
        .arg("remote.origin.url")
        .current_dir(dir)
        .output()
        .await
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

/// Run `git log -1 --format=%cI` in `dir`.
async fn git_last_commit(dir: &std::path::Path) -> Option<String> {
    let out = tokio::process::Command::new("git")
        .arg("log")
        .arg("-1")
        .arg("--format=%cI")
        .current_dir(dir)
        .output()
        .await
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

/// Scan `github_dir` for depth-1 git repos.
#[tauri::command]
pub async fn discover_repos(app_handle: AppHandle) -> Result<Vec<RepoInfo>, AppError> {
    let root = github_dir(&app_handle);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&root)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() || !path.join(".git").exists() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let remote = git_remote(&path).await;
        let last_commit_at = git_last_commit(&path).await;
        out.push(RepoInfo {
            path: path.to_string_lossy().to_string(),
            name,
            remote,
            last_commit_at,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::resolve_github_dir;
    use std::path::{Path, PathBuf};

    #[test]
    fn uses_the_configured_github_dir() {
        let home = Path::new("/home/tester");
        assert_eq!(
            resolve_github_dir(Some("/Volumes/work/repos"), Some(home)),
            PathBuf::from("/Volumes/work/repos")
        );
    }

    #[test]
    fn falls_back_to_home_documents_github() {
        let home = Path::new("/home/tester");
        assert_eq!(
            resolve_github_dir(None, Some(home)),
            PathBuf::from("/home/tester/Documents/Github")
        );
    }

    #[test]
    fn an_unset_config_field_falls_back_rather_than_scanning_cwd() {
        // `AppConfig::default()` leaves `github_dir` as "" — resolving that to a
        // relative path would make `discover_repos` scan the process cwd.
        let home = Path::new("/home/tester");
        let fallback = PathBuf::from("/home/tester/Documents/Github");
        assert_eq!(resolve_github_dir(Some(""), Some(home)), fallback);
        assert_eq!(resolve_github_dir(Some("  "), Some(home)), fallback);
    }

    #[test]
    fn a_configured_dir_wins_even_without_a_home() {
        assert_eq!(
            resolve_github_dir(Some("/srv/repos"), None),
            PathBuf::from("/srv/repos")
        );
    }

    #[test]
    fn no_config_and_no_home_yields_an_empty_path() {
        // `discover_repos` treats a non-existent root as "no repos", so an empty
        // path degrades to an empty list instead of an error.
        assert_eq!(resolve_github_dir(None, None), PathBuf::new());
    }
}
