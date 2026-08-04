//! Tauri IPC command handlers.
//!
//! The full 26-command inventory is defined in `docs/tauri/02-ipc-contracts.md`.
//! Shared DTOs live in [`types`]; the actual command fns live in the domain
//! submodules and are re-exported here for `generate_handler!`.

pub mod data_sources;
pub mod llm;
pub mod pipeline;
pub mod repos;
pub mod settings;
pub mod standup;
pub mod types;

use autostand_core::host;
use tauri_plugin_store::StoreExt;

use crate::error::AppError;

/// Resolve the per-platform autostand state directory.
pub(crate) fn state_dir() -> std::path::PathBuf {
    dirs::state_dir()
        .or_else(dirs::config_dir)
        .map_or_else(|| std::path::PathBuf::from(".autostand"), |d| d.join("autostand"))
}

/// Resolve the config store path used by `tauri-plugin-store`.
const CONFIG_STORE_PATH: &str = "config.json";
/// Key under which `AppConfig` is persisted inside the store.
const CONFIG_STORE_KEY: &str = "config";

/// Load the persisted `AppConfig` from the Tauri Store, or default if absent.
pub(crate) fn load_config(app: &impl tauri::Manager<tauri::Wry>) -> Result<types::AppConfig, AppError> {
    let store = app
        .store(CONFIG_STORE_PATH)
        .map_err(|e| AppError::Config(format!("open store: {e}")))?;
    match store.get(CONFIG_STORE_KEY) {
        Some(value) => serde_json::from_value::<types::AppConfig>(value).map_err(AppError::from),
        None => Ok(types::AppConfig::default()),
    }
}

/// Persist `config` to the Tauri Store.
pub(crate) fn save_config(app: &impl tauri::Manager<tauri::Wry>, config: &types::AppConfig) -> Result<(), AppError> {
    let store = app
        .store(CONFIG_STORE_PATH)
        .map_err(|e| AppError::Config(format!("open store: {e}")))?;
    let value = serde_json::to_value(config).map_err(AppError::from)?;
    store.set(CONFIG_STORE_KEY, value);
    store
        .save()
        .map_err(|e| AppError::Config(format!("save store: {e}")))
}

/// Get the host slug (detect + persist if not already set).
#[tauri::command]
pub async fn get_host_slug() -> Result<String, AppError> {
    let state_dir = state_dir();
    host::load_or_detect(&state_dir).map_err(|e| AppError::NotFound(e.to_string()))
}

/// Manually override the host slug.
#[tauri::command]
pub async fn set_host_slug(slug: String) -> Result<(), AppError> {
    if !host::is_valid_slug(&slug) {
        return Err(AppError::Invalid(
            "invalid host slug (rejects empty/numeric/IP-like)".into(),
        ));
    }
    let state_dir = state_dir();
    std::fs::create_dir_all(&state_dir)?;
    std::fs::write(state_dir.join("host-id"), slug)?;
    Ok(())
}

/// Load the app config from the store plugin (defaults if nothing stored).
#[tauri::command]
pub async fn get_config(app_handle: tauri::AppHandle) -> Result<types::AppConfig, AppError> {
    load_config(&app_handle)
}

/// Persist the app config to the store plugin.
#[tauri::command]
pub async fn set_config(app_handle: tauri::AppHandle, config: types::AppConfig) -> Result<(), AppError> {
    save_config(&app_handle, &config)
}