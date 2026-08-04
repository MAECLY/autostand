//! Repo discovery IPC command.
//!
//! See `docs/tauri/02-ipc-contracts.md` row `discover_repos`.

use std::path::PathBuf;

use tauri::AppHandle;

use crate::commands::{load_config, types::RepoInfo};
use crate::error::AppError;

/// Resolve the github_dir from config (default `~/Documents/Github`).
fn github_dir(app: &impl tauri::Manager<tauri::Wry>) -> PathBuf {
    let configured = load_config(app)
        .ok()
        .map(|c| c.github_dir)
        .filter(|s| !s.is_empty());
    match configured {
        Some(dir) => PathBuf::from(dir),
        None => dirs::home_dir()
            .map(|h| h.join("Documents").join("Github"))
            .unwrap_or_default(),
    }
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