//! Settings-path IPC commands.
//!
//! See `docs/tauri/02-ipc-contracts.md` rows `get_settings_paths`, `validate_paths`.

use std::path::PathBuf;

use tauri::AppHandle;

use crate::commands::{load_config, state_dir, types::PathValidation, types::SettingsPaths};
use crate::error::AppError;

/// Resolve `~/.local/share/opencode` (Linux/BSD) or `~/.config/opencode` (macOS).
fn opencode_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        dirs::home_dir()
            .map(|h| h.join(".config").join("opencode"))
            .unwrap_or_default()
    } else {
        dirs::data_local_dir()
            .map(|d| d.join("opencode"))
            .unwrap_or_default()
    }
}

/// Resolve all configured paths.
pub(crate) fn resolve_paths(app: &impl tauri::Manager<tauri::Wry>) -> SettingsPaths {
    let config = load_config(app).ok();
    let github_dir = config
        .as_ref()
        .map(|c| c.github_dir.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join("Documents").join("Github"))
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
    let dailies_dir = config
        .as_ref()
        .map(|c| c.dailies_dir.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join("Sync").join("Github_Dailies").join("dailies"))
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
    let claude_dir = dirs::home_dir()
        .map(|h| h.join(".claude"))
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let codex_dir = dirs::home_dir()
        .map(|h| h.join(".codex"))
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let gemini_dir = dirs::home_dir()
        .map(|h| h.join(".gemini"))
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let opencode_dir = opencode_dir().to_string_lossy().to_string();
    let state_dir_path = state_dir().to_string_lossy().to_string();
    let config_dir = dirs::config_dir()
        .map(|d| d.join("autostand"))
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let audit_dir = state_dir()
        .join("audit")
        .to_string_lossy()
        .to_string();
    SettingsPaths {
        github_dir,
        dailies_dir,
        claude_dir,
        codex_dir,
        gemini_dir,
        opencode_dir,
        state_dir: state_dir_path,
        config_dir,
        audit_dir,
    }
}

/// Return all configured paths.
#[tauri::command]
pub async fn get_settings_paths(app_handle: AppHandle) -> Result<SettingsPaths, AppError> {
    Ok(resolve_paths(&app_handle))
}

/// Check each path exists + is readable.
#[tauri::command]
pub async fn validate_paths(app_handle: AppHandle) -> Result<Vec<PathValidation>, AppError> {
    let paths = resolve_paths(&app_handle);
    let items = [
        ("github_dir", &paths.github_dir),
        ("dailies_dir", &paths.dailies_dir),
        ("claude_dir", &paths.claude_dir),
        ("codex_dir", &paths.codex_dir),
        ("gemini_dir", &paths.gemini_dir),
        ("opencode_dir", &paths.opencode_dir),
        ("state_dir", &paths.state_dir),
        ("config_dir", &paths.config_dir),
        ("audit_dir", &paths.audit_dir),
    ];
    let mut out = Vec::with_capacity(items.len());
    for (label, path) in items {
        let exists = std::path::Path::new(path).exists();
        let readable = exists && std::fs::metadata(path).map_or(false, |m| !m.permissions().readonly() || m.is_dir());
        let message = if exists {
            None
        } else {
            Some(format!("path does not exist: {path}"))
        };
        out.push(PathValidation {
            path: path.clone(),
            label: label.to_string(),
            exists,
            readable,
            message,
        });
    }
    Ok(out)
}