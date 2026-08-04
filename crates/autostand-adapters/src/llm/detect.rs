//! Shared helper: detect a `CLI` binary on `PATH` and get its version.

use super::CliInfo;
use std::path::{Path, PathBuf};

/// Detect a CLI binary by name on PATH; return path + version if found.
pub async fn detect_cli_binary(name: &str) -> Option<CliInfo> {
    let path = which(name)?;
    detect_cli_at(&path).await
}

/// Detect a CLI at an explicit path (returns path + version).
pub async fn detect_cli_at(path: &Path) -> Option<CliInfo> {
    let version = get_version(path).await.unwrap_or_default();
    Some(CliInfo {
        path: path.to_path_buf(),
        version,
    })
}

fn which(name: &str) -> Option<PathBuf> {
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

async fn get_version(path: &Path) -> Option<String> {
    // `AUTOSTAND_RENDER=1` on every provider-binary spawn, not just renders
    // (`AGENTS.md`: "anti-recursion env set on CLI subprocess calls"). A
    // `--version` probe writes no session today, but a provider CLI that
    // starts recording one would turn detection into work autostand ingests.
    let out = tokio::process::Command::new(path)
        .arg("--version")
        .env("AUTOSTAND_RENDER", "1")
        .output()
        .await
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}
