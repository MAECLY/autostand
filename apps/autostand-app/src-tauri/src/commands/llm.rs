//! LLM provider IPC commands.
//!
//! See `docs/tauri/02-ipc-contracts.md` rows `list_llm_providers`,
//! `test_llm_provider`, `store_api_key`, `get_api_key_status`, `detect_cli`.
//!
//! CLI detection goes through the adapter `detect_cli_binary` helper and API
//! keys through the `keyring` crate; `test_llm_provider` dispatches to the
//! adapter's `test_connection`. `list_llm_providers` still reports
//! `enabled: false` for every provider — enablement lives in `AppConfig.llm`,
//! which the Settings screen reads directly.
//!
//! Nothing in this module ever puts an API key into a return value, an error or
//! a log line: keys move keychain/env → adapter and stop there.

use std::time::Instant;

use autostand_adapters::llm::{LlmError, ProviderMode as AdapterMode};
use tauri::AppHandle;

use crate::commands::types::{
    ApiKeyMode, ApiKeyStatus, AppConfig, CliDetection, LlmProviderConfig, ProviderConfig,
    TestProviderResult,
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
        .is_some_and(|p| p.local_only)
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
        if std::env::var(var).is_ok_and(|v| !v.is_empty()) {
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
pub async fn list_llm_providers(app_handle: AppHandle) -> Result<Vec<LlmProviderConfig>, AppError> {
    let config = crate::commands::load_config(&app_handle).ok();
    let mut out = Vec::with_capacity(PROVIDERS.len());
    for def in PROVIDERS {
        let stored = config.as_ref().map_or_else(
            || ProviderConfig {
                id: def.id.to_string(),
                ..ProviderConfig::default()
            },
            |c| stored_entry(c, def.id),
        );
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
            enabled: stored.enabled,
            mode: stored.mode,
            model: stored.model,
            cli,
            api_key: resolve_api_key_status(def.id),
        });
    }
    Ok(out)
}

/// Map the IPC transport (`"cli" | "api"`) onto a single-channel adapter mode.
///
/// WHY single-channel: the adapters' `CliFirst` / `ApiFallback` modes silently
/// try the *other* transport when the first fails, so a "cli" probe could come
/// back green while the CLI is missing — and could bill the user's API key for
/// a button labelled "test the CLI".
fn probe_mode(raw: &str) -> Option<AdapterMode> {
    match raw {
        "cli" => Some(AdapterMode::CliOnly),
        "api" => Some(AdapterMode::ApiOnly),
        _ => None,
    }
}

/// Secret-free reason a connection probe failed, for the UI and the run log.
///
/// WHY not [`autostand_adapters::llm::LlmError`]'s `Display`: `ApiError` carries
/// the raw transport-error string and `CliExitError` carries stderr. Gemini
/// authenticates with the key in the URL query, so that string can contain the
/// key verbatim. This message is rendered in Settings and logged, so it is built
/// from a fixed per-variant label plus fields that can never hold a secret.
fn probe_failure(err: &LlmError) -> String {
    match err {
        LlmError::Timeout { secs } => format!("timed out after {secs}s"),
        LlmError::CliNotFound { .. } => "CLI binary not found".to_string(),
        LlmError::CliExitError { code, .. } => format!("CLI exited with code {code}"),
        LlmError::ApiError { status, .. } => format!("API error (HTTP {status})"),
        LlmError::AuthError => "no API key in the keychain or the environment".to_string(),
        LlmError::ParseError { .. } => "could not parse the provider response".to_string(),
        LlmError::RateLimit { retry_after_secs } => retry_after_secs.map_or_else(
            || "rate limited".to_string(),
            |secs| format!("rate limited (retry after {secs}s)"),
        ),
    }
}

/// The stored settings for `provider`, or a blank entry when it has none yet.
///
/// Defaulting rather than failing is what lets a provider be probed straight
/// from a fresh install: `render::provider_config` substitutes the documented
/// per-provider model, base URL and timeout for every blank field.
fn stored_entry(config: &AppConfig, provider: &str) -> ProviderConfig {
    config
        .llm
        .providers
        .iter()
        .find(|p| p.id == provider)
        .cloned()
        .unwrap_or_else(|| ProviderConfig {
            id: provider.to_string(),
            ..ProviderConfig::default()
        })
}

/// Test a provider connection over one transport (`mode` is `"cli"` or `"api"`).
///
/// Dispatches to the adapter's `test_connection` with the provider's stored
/// settings (model, CLI path, base URL, timeout). Only the transport probe can
/// fail: a refused connection comes back as `ok: false` with a reason, never as
/// an IPC error, so the Settings card can show it inline.
///
/// The API key is deliberately *not* resolved here — the adapter resolves it
/// itself, and only on its API path, so a `"cli"` probe never unlocks the
/// keychain.
///
/// # Errors
///
/// [`AppError::Invalid`] when `provider` is not one of the five documented ids,
/// and [`AppError::Config`] when the config store cannot be read.
#[tauri::command]
pub async fn test_llm_provider(
    app_handle: AppHandle,
    provider: String,
    mode: String,
) -> Result<TestProviderResult, AppError> {
    if binary_for(&provider).is_none() {
        return Err(AppError::Invalid(format!("unknown provider: {provider}")));
    }
    let start = Instant::now();
    let Some(probe) = probe_mode(&mode) else {
        return Ok(TestProviderResult {
            ok: false,
            message: format!("unknown mode: {mode} (expected cli|api)"),
            latency_ms: elapsed_ms(start),
        });
    };
    // `binary_for` already accepted the id, so the registry must know it too;
    // treat a mismatch as the same invalid-provider error rather than panicking.
    let adapter = crate::render::adapter_for(&provider)
        .ok_or_else(|| AppError::Invalid(format!("unknown provider: {provider}")))?;

    let app_config = crate::commands::load_config(&app_handle)?;
    let mut adapter_config =
        crate::render::provider_config(&stored_entry(&app_config, &provider), None);
    adapter_config.mode = probe;

    match adapter.test_connection(&adapter_config).await {
        Ok(result) => Ok(TestProviderResult {
            ok: result.ok,
            message: result.message,
            latency_ms: result.latency_ms,
        }),
        Err(err) => {
            let message = probe_failure(&err);
            tracing::warn!(
                provider = %provider,
                transport = %mode,
                reason = %message,
                "provider connection test failed"
            );
            Ok(TestProviderResult {
                ok: false,
                message,
                latency_ms: elapsed_ms(start),
            })
        }
    }
}

/// Milliseconds since `start`, saturating instead of wrapping.
fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Store an API key in the OS keychain under `autostand.<provider>`.
#[tauri::command]
pub async fn store_api_key(
    _app_handle: AppHandle,
    provider: String,
    key: String,
) -> Result<(), AppError> {
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
pub async fn get_api_key_status(
    _app_handle: AppHandle,
    provider: String,
) -> Result<ApiKeyStatus, AppError> {
    if binary_for(&provider).is_none() {
        return Err(AppError::Invalid(format!("unknown provider: {provider}")));
    }
    Ok(resolve_api_key_status(&provider))
}

/// Detect a provider's CLI binary on PATH and probe `--version`.
#[tauri::command]
pub async fn detect_cli(
    _app_handle: AppHandle,
    provider: String,
) -> Result<CliDetection, AppError> {
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

#[cfg(test)]
mod tests {
    use super::{
        binary_for, env_var_for, is_local_only, probe_failure, probe_mode, stored_entry,
        AdapterMode, AppConfig, LlmError, ProviderConfig, PROVIDERS,
    };
    use crate::commands::types::ProviderMode;

    #[test]
    fn exposes_the_five_documented_providers() {
        let ids: Vec<&str> = PROVIDERS.iter().map(|p| p.id).collect();
        assert_eq!(ids, ["claude", "ollama", "openai", "gemini", "grok"]);
    }

    #[test]
    fn maps_provider_ids_to_cli_binaries() {
        assert_eq!(binary_for("claude"), Some("claude"));
        assert_eq!(binary_for("openai"), Some("codex"));
        assert_eq!(binary_for("nope"), None);
    }

    #[test]
    fn unknown_providers_have_no_binary_so_commands_reject_them() {
        // Every LLM command gates on `binary_for(..).is_none()`.
        for provider in ["", "Claude", "anthropic", "gpt"] {
            assert!(binary_for(provider).is_none(), "{provider} must be unknown");
        }
    }

    #[test]
    fn only_ollama_is_local_only() {
        assert!(is_local_only("ollama"));
        for provider in ["claude", "openai", "gemini", "grok"] {
            assert!(!is_local_only(provider), "{provider} needs an API key");
        }
        assert!(!is_local_only("nope"));
    }

    #[test]
    fn api_providers_declare_an_env_var_and_ollama_does_not() {
        assert_eq!(env_var_for("claude"), Some("ANTHROPIC_API_KEY"));
        assert_eq!(env_var_for("openai"), Some("OPENAI_API_KEY"));
        assert_eq!(env_var_for("gemini"), Some("GEMINI_API_KEY"));
        assert_eq!(env_var_for("grok"), Some("XAI_API_KEY"));
        assert_eq!(env_var_for("ollama"), None);
        assert_eq!(env_var_for("nope"), None);
    }

    // ---- connection probe -------------------------------------------------

    #[test]
    fn the_two_contract_transports_map_to_single_channel_modes() {
        assert_eq!(probe_mode("cli"), Some(AdapterMode::CliOnly));
        assert_eq!(probe_mode("api"), Some(AdapterMode::ApiOnly));
    }

    #[test]
    fn no_other_transport_is_accepted() {
        // A typo must surface as `ok: false`, never as a mode that falls back
        // to the other transport.
        for raw in ["", "CLI", "Api", "cli-first", "clifirst", "auto"] {
            assert!(probe_mode(raw).is_none(), "{raw:?} must be rejected");
        }
    }

    #[test]
    fn probe_failures_never_carry_the_transport_payload() {
        // Gemini authenticates with `?key=<secret>` in the URL, so reqwest's
        // error string — and therefore `ApiError.body` — can contain the key.
        let secret = "AIzaSy-not-a-real-key";
        let leaky = [
            LlmError::ApiError {
                status: 400,
                body: format!("error sending request for url (https://x/v1beta?key={secret})"),
            },
            LlmError::CliExitError {
                code: 2,
                stderr: format!("bad credentials: {secret}"),
            },
            LlmError::ParseError {
                raw: secret.to_string(),
            },
        ];
        for err in &leaky {
            let message = probe_failure(err);
            assert!(
                !message.contains(secret),
                "probe message leaked the payload: {message:?}"
            );
        }
    }

    #[test]
    fn probe_failures_keep_the_non_secret_detail() {
        assert_eq!(
            probe_failure(&LlmError::Timeout { secs: 15 }),
            "timed out after 15s"
        );
        assert_eq!(
            probe_failure(&LlmError::ApiError {
                status: 503,
                body: String::new()
            }),
            "API error (HTTP 503)"
        );
        assert_eq!(
            probe_failure(&LlmError::CliExitError {
                code: 127,
                stderr: String::new()
            }),
            "CLI exited with code 127"
        );
        assert_eq!(
            probe_failure(&LlmError::RateLimit {
                retry_after_secs: Some(30)
            }),
            "rate limited (retry after 30s)"
        );
        assert_eq!(
            probe_failure(&LlmError::RateLimit {
                retry_after_secs: None
            }),
            "rate limited"
        );
        assert_eq!(
            probe_failure(&LlmError::CliNotFound { searched: vec![] }),
            "CLI binary not found"
        );
        assert_eq!(
            probe_failure(&LlmError::AuthError),
            "no API key in the keychain or the environment"
        );
    }

    #[test]
    fn stored_settings_win_when_the_provider_is_configured() {
        let mut config = AppConfig::default();
        config.llm.providers = vec![ProviderConfig {
            id: "ollama".to_string(),
            enabled: true,
            mode: ProviderMode::ApiOnly,
            model: "llama3.3".to_string(),
            api_base_url: Some("http://localhost:11500".to_string()),
            timeout_secs: 42,
            ..ProviderConfig::default()
        }];
        let entry = stored_entry(&config, "ollama");
        assert_eq!(entry.model, "llama3.3");
        assert_eq!(
            entry.api_base_url.as_deref(),
            Some("http://localhost:11500")
        );
        assert_eq!(entry.timeout_secs, 42);
    }

    #[test]
    fn an_unconfigured_provider_probes_with_a_blank_entry() {
        // A fresh install has no `llm.providers` at all; the probe must still
        // run, with `render::provider_config` filling in the defaults.
        let entry = stored_entry(&AppConfig::default(), "claude");
        assert_eq!(entry.id, "claude");
        assert!(entry.model.is_empty());
        assert!(entry.api_base_url.is_none());
        assert_eq!(entry.timeout_secs, 0);
    }

    #[test]
    fn a_stored_entry_never_carries_a_key_only_a_reference() {
        // `api_key_ref` is a keychain *name*; the key itself must never be in
        // the config store, so there is no field on the DTO that could hold it.
        let mut config = AppConfig::default();
        config.llm.providers = vec![ProviderConfig {
            id: "claude".to_string(),
            api_key_ref: Some("autostand.claude".to_string()),
            ..ProviderConfig::default()
        }];
        let entry = stored_entry(&config, "claude");
        let value = serde_json::to_value(&entry).expect("ProviderConfig serializes");
        let object = value.as_object().expect("object");
        assert!(!object.contains_key("api_key"), "{object:?}");
        assert_eq!(object["api_key_ref"], serde_json::json!("autostand.claude"));
    }
}
