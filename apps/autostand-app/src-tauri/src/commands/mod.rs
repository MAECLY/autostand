//! Tauri IPC command handlers.
//!
//! The full command inventory is defined in `docs/tauri/02-ipc-contracts.md`.
//! Shared DTOs live in [`types`]; the actual command fns live in the domain
//! submodules and are registered with `generate_handler!` in `lib.rs`.

pub mod data_sources;
pub mod llm;
pub mod pipeline;
pub mod repos;
pub mod settings;
pub mod standup;
pub mod sync;
pub mod types;

use std::path::{Path, PathBuf};

use autostand_core::host;
use tauri_plugin_store::StoreExt;

use crate::error::AppError;

/// Resolve the per-platform autostand state directory.
pub(crate) fn state_dir() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::config_dir)
        .map_or_else(|| PathBuf::from(".autostand"), |d| d.join("autostand"))
}

/// Resolve a configured directory string, falling back to `home/<fallback…>`.
///
/// A blank configured value counts as unset: the Settings UI clears a text field
/// to `""` rather than to `null`, and treating that as a literal path would send
/// the whole app scanning the process's working directory.
pub(crate) fn resolve_dir(
    configured: Option<&str>,
    home: Option<&Path>,
    fallback: &[&str],
) -> PathBuf {
    if let Some(dir) = configured.map(str::trim).filter(|s| !s.is_empty()) {
        return PathBuf::from(dir);
    }
    home.map(|h| {
        fallback
            .iter()
            .fold(h.to_path_buf(), |acc, segment| acc.join(segment))
    })
    .unwrap_or_default()
}

/// Reject host slugs the App Script would reject (empty, numeric, IP-like).
///
/// Kept separate from the command so the rule is testable without a Tauri app;
/// slug stability is a hard invariant (a DHCP-derived slug silently forks a
/// host's AUTO block into a second one).
pub(crate) fn validate_host_slug(slug: &str) -> Result<(), AppError> {
    if host::is_valid_slug(slug) {
        Ok(())
    } else {
        Err(AppError::Invalid(
            "invalid host slug (rejects empty/numeric/IP-like)".into(),
        ))
    }
}

/// Resolve the config store path used by `tauri-plugin-store`.
const CONFIG_STORE_PATH: &str = "config.json";
/// Key under which `AppConfig` is persisted inside the store.
const CONFIG_STORE_KEY: &str = "config";

/// Load the persisted `AppConfig` from the Tauri Store, or default if absent.
pub(crate) fn load_config(
    app: &impl tauri::Manager<tauri::Wry>,
) -> Result<types::AppConfig, AppError> {
    let store = app
        .store(CONFIG_STORE_PATH)
        .map_err(|e| AppError::Config(format!("open store: {e}")))?;
    match store.get(CONFIG_STORE_KEY) {
        Some(value) => serde_json::from_value::<types::AppConfig>(value).map_err(AppError::from),
        None => Ok(types::AppConfig::default()),
    }
}

/// Persist `config` to the Tauri Store.
pub(crate) fn save_config(
    app: &impl tauri::Manager<tauri::Wry>,
    config: &types::AppConfig,
) -> Result<(), AppError> {
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
    validate_host_slug(&slug)?;
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
pub async fn set_config(
    app_handle: tauri::AppHandle,
    config: types::AppConfig,
) -> Result<(), AppError> {
    save_config(&app_handle, &config)
}

#[cfg(test)]
mod tests {
    use super::{resolve_dir, validate_host_slug};
    use crate::error::AppError;
    use std::path::{Path, PathBuf};

    #[test]
    fn accepts_a_normal_host_slug() {
        assert!(validate_host_slug("MacStudio-de-Miguel").is_ok());
        assert!(validate_host_slug("workstation-01").is_ok());
    }

    #[test]
    fn rejects_empty_numeric_and_ip_like_slugs() {
        for slug in ["", "   ", "192", "12345", "192.168.1.1", "10.0.0.1"] {
            assert!(
                validate_host_slug(slug).is_err(),
                "slug {slug:?} must be rejected"
            );
        }
    }

    #[test]
    fn rejected_slugs_report_the_invalid_code() {
        let err = validate_host_slug("10.0.0.1").expect_err("IP-like slug");
        assert!(matches!(err, AppError::Invalid(_)));
        assert_eq!(
            serde_json::to_value(&err).unwrap()["code"],
            serde_json::json!("invalid")
        );
    }

    #[test]
    fn resolve_dir_prefers_the_configured_value() {
        let home = PathBuf::from("/home/tester");
        assert_eq!(
            resolve_dir(Some("/srv/repos"), Some(&home), &["Documents", "Github"]),
            PathBuf::from("/srv/repos")
        );
    }

    #[test]
    fn resolve_dir_treats_blank_configuration_as_unset() {
        let home = PathBuf::from("/home/tester");
        let expected = home.join("Documents").join("Github");
        for configured in [None, Some(""), Some("   ")] {
            assert_eq!(
                resolve_dir(configured, Some(&home), &["Documents", "Github"]),
                expected,
                "configured {configured:?} must fall back"
            );
        }
    }

    #[test]
    fn resolve_dir_trims_the_configured_value() {
        assert_eq!(
            resolve_dir(Some("  /srv/repos  "), None, &["Documents"]),
            PathBuf::from("/srv/repos")
        );
    }

    #[test]
    fn resolve_dir_without_a_home_yields_an_empty_path() {
        assert_eq!(
            resolve_dir(None, None, &["Documents", "Github"]),
            PathBuf::new()
        );
    }

    #[test]
    fn resolve_dir_joins_every_fallback_segment_in_order() {
        let home = Path::new("/home/tester");
        assert_eq!(
            resolve_dir(None, Some(home), &["Sync", "Github_Dailies", "dailies"]),
            PathBuf::from("/home/tester/Sync/Github_Dailies/dailies")
        );
    }
}
