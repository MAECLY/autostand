//! Tauri IPC command handlers.
//!
//! See `docs/tauri/02-ipc-contracts.md` for the full command inventory.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Resolve the per-platform autostand state directory.
fn state_dir() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::config_dir)
        .map_or_else(|| PathBuf::from(".autostand"), |d| d.join("autostand"))
}

/// App configuration (stub — full schema in `docs/specs/configuration.md`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub github_dir: String,
    pub dailies_dir: String,
    pub host_slug_override: Option<String>,
    pub render_mode: String,
}

/// Result of a compile run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileResult {
    pub ok: bool,
    pub message: String,
    pub files_touched: Vec<String>,
}

/// Get the host slug (detect + persist if not already set).
#[tauri::command]
pub async fn get_host_slug() -> Result<String, String> {
    let state_dir = state_dir();
    autostand_core::host::load_or_detect(&state_dir).map_err(|e| e.to_string())
}

/// Manually override the host slug.
#[tauri::command]
pub async fn set_host_slug(slug: String) -> Result<(), String> {
    if !autostand_core::host::is_valid_slug(&slug) {
        return Err("invalid host slug (rejects empty/numeric/IP-like)".into());
    }
    let state_dir = state_dir();
    std::fs::create_dir_all(&state_dir).map_err(|e| e.to_string())?;
    std::fs::write(state_dir.join("host-id"), slug).map_err(|e| e.to_string())
}

/// Load the app config from the store plugin.
#[tauri::command]
pub async fn get_config() -> Result<AppConfig, String> {
    Ok(AppConfig::default())
}

/// Persist the app config.
#[tauri::command]
pub async fn set_config(_config: AppConfig) -> Result<(), String> {
    Ok(())
}

/// Run the full pipeline for a date (default: today).
#[tauri::command]
pub async fn compile_standup(_date: Option<String>) -> Result<CompileResult, String> {
    let today = chrono::Local::now().date_naive();
    let (f_today, _f_prev) = autostand_scheduler::selfheal::compute_targets(today);
    autostand_core::pipeline::compile_file(f_today).map_err(|e| e.to_string())?;
    Ok(CompileResult {
        ok: true,
        message: format!("compiled {f_today}"),
        files_touched: vec![],
    })
}

/// Append an item to the MANUAL region of a standup file.
#[tauri::command]
pub async fn add_manual_item(_date: String, _item: String) -> Result<(), String> {
    // TODO: fileops add-manual (atomic write-then-rename)
    Ok(())
}

/// Trigger a run immediately.
#[tauri::command]
pub async fn trigger_run_now() -> Result<CompileResult, String> {
    compile_standup(None).await
}
