//! Shared helper: detect a `CLI` binary on `PATH` and get its version.

use super::CliInfo;
use std::path::PathBuf;

/// Detect a CLI binary by name on PATH; return path + version if found.
pub async fn detect_cli_binary(name: &str) -> Option<CliInfo> {
    let path = which(name)?;
    let version = get_version(&path).await.unwrap_or_default();
    Some(CliInfo { path, version })
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

async fn get_version(path: &std::path::Path) -> Option<String> {
    let out = tokio::process::Command::new(path)
        .arg("--version")
        .output()
        .await
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}
