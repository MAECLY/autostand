//! LLM provider IPC commands.
//!
//! See `docs/tauri/02-ipc-contracts.md` rows `list_llm_providers`,
//! `test_llm_provider`, `store_api_key`, `get_api_key_status`, `detect_cli`.
//!
//! These are functional stubs: they return the right shapes with sensible
//! defaults (providers `enabled: false`, CLI detection via the adapter
//! `detect_cli_binary` helper, keyring via the `keyring` crate).

use std::time::Instant;

use tauri::AppHandle;

use crate::commands::types::{
    ApiKeyMode, ApiKeyStatus, CliDetection, LlmProviderConfig, ProviderMode, TestProviderResult,
};
use crate::error::AppError;

/// Provider descriptor used by `list_llm_providers`.
struct ProviderDef {
    id: &'static str,
    label: &'static str,
    binary: &'static str,
    env_var: Option<&'static str>,
    local_only: bool,
}

/// The 5 providers per `docs/llm-adapters/`.
const PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: "claude",
        label: "Claude (Anthropic)",
        binary: "claude",
        env_var: Some("ANTHROPIC_API_KEY"),
        local_only: false,
    },
    ProviderDef {
        id: "ollama",
        label: "Ollama",
        binary: "ollama",
        env_var: None,
        local_only: true,
    },
    ProviderDef {
        id: "openai",
        label: "OpenAI / Codex",
        binary: "codex",
        env_var: Some("OPENAI_API_KEY"),
        local_only: false,
    },
    ProviderDef {
        id: "gemini",
        label: "Gemini (Google)",
        binary: "gemini",
        env_var: Some("GEMINI_API_KEY"),
        local_only: false,
    },
    ProviderDef {
        id: "grok",
        label: "Grok (xAI)",
        binary: "grok",
        env_var: Some("XAI_API_KEY"),
        local_only: false,
    },
];

/// Map a provider id to its binary name (or `None` if unknown).
fn binary_for(provider: &str) -> Option<&'static str> {
    PROVIDERS
        .iter()
        .find(|p| p.id == provider)
        .map(|p| p.binary)
}

/// Map a provider id to its API key env var (or `None` for local-only providers).
fn env_var_for(provider: &str) -> Option<&'static str> {
    PROVIDERS
        .iter()
        .find(|p| p.id == provider)
        .and_then(|p| p.env_var)
}

/// Whether a provider is local-only (no API key).
fn is_local_only(provider: &str) -> bool {
    PROVIDERS
        .iter()
        .find(|p| p.id == provider)
        .map_or(false, |p| p.local_only)
}

/// Resolve the API key status for a provider (keychain → env → none).
fn resolve_api_key_status(provider: &str) -> ApiKeyStatus {
    if is_local_only(provider) {
        return ApiKeyStatus {
            set: true,
            mode: ApiKeyMode::None,
        };
    }
    if let Ok(entry) = keyring::Entry::new("autostand", provider) {
        if entry.get_password().is_ok() {
            return ApiKeyStatus {
                set: true,
                mode: ApiKeyMode::Keychain,
            };
        }
    }
    if let Some(var) = env_var_for(provider) {
        if std::env::var(var).map_or(false, |v| !v.is_empty()) {
            return ApiKeyStatus {
                set: true,
                mode: ApiKeyMode::Env,
            };
        }
    }
    ApiKeyStatus::default()
}

/// List the 5 providers with CLI/API-key status.
#[tauri::command]
pub async fn list_llm_providers(_app_handle: AppHandle) -> Result<Vec<LlmProviderConfig>, AppError> {
    let mut out = Vec::with_capacity(PROVIDERS.len());
    for def in PROVIDERS {
        let cli = autostand_adapters::llm::detect_cli_binary(def.binary)
            .await
            .map(|info| CliDetection {
                found: true,
                path: info.path.to_string_lossy().to_string(),
                version: info.version,
            })
            .unwrap_or_default();
        out.push(LlmProviderConfig {
            id: def.id.to_string(),
            label: def.label.to_string(),
            enabled: false,
            mode: ProviderMode::CliFirst,
            model: String::new(),
            cli,
            api_key: resolve_api_key_status(def.id),
        });
    }
    Ok(out)
}

/// Test a provider connection (CLI or API depending on `mode`).
///
/// This is a stub: it returns `ok: true, message: "not wired"` with a tiny
/// measured latency. The real implementation will dispatch to the adapter's
/// `test_connection` once provider config is wired into `AppConfig.llm`.
#[tauri::command]
pub async fn test_llm_provider(
    _app_handle: AppHandle,
    provider: String,
    mode: String,
) -> Result<TestProviderResult, AppError> {
    if binary_for(&provider).is_none() {
        return Err(AppError::Invalid(format!("unknown provider: {provider}")));
    }
    let start = Instant::now();
    let ok = matches!(mode.as_str(), "cli" | "api");
    let message = if ok {
        "test not yet wired".to_string()
    } else {
        format!("unknown mode: {mode} (expected cli|api)")
    };
    Ok(TestProviderResult {
        ok,
        message,
        latency_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(0),
    })
}

/// Store an API key in the OS keychain under `autostand.<provider>`.
#[tauri::command]
pub async fn store_api_key(_app_handle: AppHandle, provider: String, key: String) -> Result<(), AppError> {
    if binary_for(&provider).is_none() {
        return Err(AppError::Invalid(format!("unknown provider: {provider}")));
    }
    if is_local_only(&provider) {
        return Err(AppError::Invalid(format!(
            "provider {provider} is local-only and does not use an API key"
        )));
    }
    let entry = keyring::Entry::new("autostand", &provider)
        .map_err(|e| AppError::Config(format!("keyring entry: {e}")))?;
    entry
        .set_password(&key)
        .map_err(|e| AppError::Config(format!("keyring set: {e}")))
}

/// Get the API key status for a provider.
#[tauri::command]
pub async fn get_api_key_status(_app_handle: AppHandle, provider: String) -> Result<ApiKeyStatus, AppError> {
    if binary_for(&provider).is_none() {
        return Err(AppError::Invalid(format!("unknown provider: {provider}")));
    }
    Ok(resolve_api_key_status(&provider))
}

/// Detect a provider's CLI binary on PATH and probe `--version`.
#[tauri::command]
pub async fn detect_cli(_app_handle: AppHandle, provider: String) -> Result<CliDetection, AppError> {
    let binary = binary_for(&provider)
        .ok_or_else(|| AppError::Invalid(format!("unknown provider: {provider}")))?;
    let info = autostand_adapters::llm::detect_cli_binary(binary)
        .await
        .map(|i| CliDetection {
            found: true,
            path: i.path.to_string_lossy().to_string(),
            version: i.version,
        })
        .unwrap_or_default();
    Ok(info)
}