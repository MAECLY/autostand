//! LLM provider IPC commands.
//!
//! See `docs/tauri/02-ipc-contracts.md` rows `list_llm_providers`,
//! `test_llm_provider`, `list_provider_models`, `store_api_key`,
//! `get_api_key_status`, `detect_cli`.
//!
//! CLI detection goes through the adapter `detect_cli_binary` helper and API
//! keys through the `keyring` crate; `test_llm_provider` dispatches to the
//! adapter's `test_connection`. `list_llm_providers` still reports
//! `enabled: false` for every provider — enablement lives in `AppConfig.llm`,
//! which the Settings screen reads directly.
//!
//! Nothing in this module ever puts an API key into a return value, an error or
//! a log line: keys move keychain/env → adapter and stop there.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use autostand_adapters::llm::{LlmError, ProviderMode as AdapterMode};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::commands::types::{
    ApiKeyMode, ApiKeyStatus, AppConfig, CliDetection, LlmProviderConfig, ProviderAvailability,
    ProviderConfig, ProviderHealth, TestProviderResult, UsageSource, UsageWindow,
};
use crate::error::AppError;
use crate::render::{ProviderAttempt, ProviderAttemptStatus};

/// Provider descriptor used by `list_llm_providers`.
struct ProviderDef {
    id: &'static str,
    label: &'static str,
    binary: &'static str,
    env_var: Option<&'static str>,
    local_only: bool,
}

/// Providers exposed to Settings and the failover chain.
const PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: "builtin-local",
        label: "Built-in Local AI",
        binary: "autostand-local-llm",
        env_var: None,
        local_only: true,
    },
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

const CODEX_HEALTH_TIMEOUT: Duration = Duration::from_secs(8);

static INFERRED_HEALTH: OnceLock<Mutex<HashMap<String, ProviderHealth>>> = OnceLock::new();

fn inferred_health() -> &'static Mutex<HashMap<String, ProviderHealth>> {
    INFERRED_HEALTH.get_or_init(|| Mutex::new(HashMap::new()))
}

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

/// List providers with CLI/API-key status.
#[tauri::command]
pub async fn list_llm_providers(app_handle: AppHandle) -> Result<Vec<LlmProviderConfig>, AppError> {
    let config = crate::commands::load_config(&app_handle).ok();
    let mut out = Vec::with_capacity(PROVIDERS.len());
    for def in PROVIDERS {
        let mut stored = config.as_ref().map_or_else(
            || ProviderConfig {
                id: def.id.to_string(),
                ..ProviderConfig::default()
            },
            |c| stored_entry(c, def.id),
        );
        if def.id == "builtin-local" {
            stored.mode = crate::commands::types::ProviderMode::CliOnly;
            if let Some(selected) = crate::commands::local_models::selected_model_id(&app_handle) {
                stored.model = selected;
                stored.enabled = true;
            }
        }
        let cli = if let Some(adapter) = crate::render::adapter_for(def.id) {
            adapter.detect_cli().await
        } else {
            None
        }
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
    if is_local_only(&provider) && probe == AdapterMode::ApiOnly {
        return Ok(TestProviderResult {
            ok: false,
            message: "This provider is local-only and has no API transport".to_string(),
            latency_ms: elapsed_ms(start),
        });
    }
    // `binary_for` already accepted the id, so the registry must know it too;
    // treat a mismatch as the same invalid-provider error rather than panicking.
    let adapter = crate::render::adapter_for(&provider)
        .ok_or_else(|| AppError::Invalid(format!("unknown provider: {provider}")))?;

    let app_config = crate::commands::load_config(&app_handle)?;
    let mut adapter_config = crate::render::provider_config_with_local_policy(
        &stored_entry(&app_config, &provider),
        None,
        app_config.llm.local_runtime_policy,
    );
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

fn unknown_health(provider: &str, reason: &str) -> ProviderHealth {
    ProviderHealth {
        provider: provider.to_string(),
        availability: ProviderAvailability::Unknown,
        source: UsageSource::Unknown,
        windows: Vec::new(),
        reason: Some(reason.to_string()),
        checked_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Retain a short-lived, secret-free availability inference from render attempts.
/// Exact quota windows are never synthesized from failures.
pub fn record_provider_attempts(attempts: &[ProviderAttempt]) {
    let mut cache = inferred_health()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for attempt in attempts {
        if attempt.status == ProviderAttemptStatus::Succeeded {
            cache.remove(&attempt.provider);
            continue;
        }
        let Some(reason) = attempt.reason.as_deref() else {
            continue;
        };
        let availability = match reason {
            "usage_balance_exhausted" | "payment_required" => ProviderAvailability::Exhausted,
            "rate_limit" => ProviderAvailability::RateLimited,
            "auth_error" | "not_logged_in" => ProviderAvailability::AuthRequired,
            "unsupported_model" => ProviderAvailability::ModelUnavailable,
            _ => ProviderAvailability::Unavailable,
        };
        cache.insert(
            attempt.provider.clone(),
            ProviderHealth {
                provider: attempt.provider.clone(),
                availability,
                source: UsageSource::FailureInferred,
                windows: Vec::new(),
                reason: Some(reason.to_string()),
                checked_at: chrono::Utc::now().to_rfc3339(),
            },
        );
    }
}

fn codex_window_id(duration_mins: Option<u64>, fallback: &str) -> String {
    match duration_mins {
        Some(300) => "five_hour".to_string(),
        Some(10_080) => "weekly".to_string(),
        _ => fallback.to_string(),
    }
}

fn parse_codex_window(id: &str, value: &serde_json::Value) -> Option<UsageWindow> {
    let used = value
        .get("usedPercent")
        .or_else(|| value.get("used_percent"))
        .and_then(serde_json::Value::as_f64)?;
    let duration = value
        .get("windowDurationMins")
        .or_else(|| value.get("window_duration_mins"))
        .and_then(serde_json::Value::as_u64);
    let resets_at = value
        .get("resetsAt")
        .or_else(|| value.get("resets_at"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map(|timestamp| timestamp.to_rfc3339());
    Some(UsageWindow {
        id: codex_window_id(duration, id),
        used_percent: Some(used),
        remaining_percent: Some((100.0 - used).clamp(0.0, 100.0)),
        resets_at,
    })
}

/// Parse only the documented rate-limit fields from a Codex app-server result.
/// Unknown fields are deliberately ignored so protocol additions do not break Settings.
fn parse_codex_health(result: &serde_json::Value) -> Option<ProviderHealth> {
    let selected = result
        .get("rateLimitsByLimitId")
        .and_then(|limits| limits.get("codex"))
        .or_else(|| result.get("rateLimits"))?;
    let mut windows = Vec::new();
    for (fallback, key) in [("primary", "primary"), ("secondary", "secondary")] {
        if let Some(window) = selected
            .get(key)
            .filter(|window| !window.is_null())
            .and_then(|window| parse_codex_window(fallback, window))
        {
            windows.push(window);
        }
    }
    if windows.is_empty() {
        return None;
    }
    let minimum = windows
        .iter()
        .filter_map(|window| window.remaining_percent)
        .fold(100.0_f64, f64::min);
    let availability = if minimum <= 0.0 {
        ProviderAvailability::Exhausted
    } else if minimum <= 20.0 {
        ProviderAvailability::Low
    } else {
        ProviderAvailability::Available
    };
    Some(ProviderHealth {
        provider: "openai".to_string(),
        availability,
        source: UsageSource::ProviderReported,
        windows,
        reason: None,
        checked_at: chrono::Utc::now().to_rfc3339(),
    })
}

async fn read_rpc_response(
    lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    id: i64,
) -> Result<serde_json::Value, ()> {
    loop {
        let line = tokio::time::timeout(CODEX_HEALTH_TIMEOUT, lines.next_line())
            .await
            .map_err(|_| ())?
            .map_err(|_| ())?
            .ok_or(())?;
        let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if message.get("id").and_then(serde_json::Value::as_i64) == Some(id) {
            return message.get("result").cloned().ok_or(());
        }
    }
}

async fn codex_health(config: &AppConfig) -> ProviderHealth {
    let entry = stored_entry(config, "openai");
    let binary = entry.cli_path.unwrap_or_else(|| "codex".into());
    let Ok(mut child) = Command::new(binary)
        .args(["app-server", "--stdio"])
        .env("AUTOSTAND_RENDER", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    else {
        return unknown_health("openai", "cli_not_found");
    };
    let Some(mut stdin) = child.stdin.take() else {
        return unknown_health("openai", "usage_probe_failed");
    };
    let Some(stdout) = child.stdout.take() else {
        return unknown_health("openai", "usage_probe_failed");
    };
    let initialize = serde_json::json!({
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": { "name": "autostand", "title": "Autostand", "version": env!("CARGO_PKG_VERSION") },
            "capabilities": {}
        }
    });
    let initialized = serde_json::json!({ "method": "initialized", "params": {} });
    let request = serde_json::json!({ "id": 2, "method": "account/rateLimits/read", "params": {} });
    let mut lines = BufReader::new(stdout).lines();
    let outcome = async {
        stdin
            .write_all(format!("{initialize}\n").as_bytes())
            .await
            .map_err(|_| ())?;
        stdin.flush().await.map_err(|_| ())?;
        let _ = read_rpc_response(&mut lines, 1).await?;
        stdin
            .write_all(format!("{initialized}\n{request}\n").as_bytes())
            .await
            .map_err(|_| ())?;
        stdin.flush().await.map_err(|_| ())?;
        read_rpc_response(&mut lines, 2).await
    }
    .await;
    let _ = child.kill().await;
    match outcome.ok().and_then(|result| parse_codex_health(&result)) {
        Some(health) => health,
        None => unknown_health("openai", "usage_unavailable"),
    }
}

async fn provider_health(config: &AppConfig, provider: &str) -> ProviderHealth {
    let reported = match provider {
        "openai" => codex_health(config).await,
        "claude" | "grok" => unknown_health(provider, "usage_not_programmatically_available"),
        _ => unknown_health(provider, "usage_not_supported"),
    };
    if reported.source != UsageSource::Unknown {
        return reported;
    }
    inferred_health()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(provider)
        .cloned()
        .unwrap_or(reported)
}

fn low_usage_notifications(
    config: &AppConfig,
    health: &[ProviderHealth],
) -> Vec<crate::notifications::SystemNotification> {
    let mut notifications = Vec::new();
    for item in health {
        for window in &item.windows {
            let Some(remaining) = window.remaining_percent else {
                continue;
            };
            if remaining > f64::from(config.notifications.low_usage_threshold_percent) {
                continue;
            }
            let remaining = remaining.clamp(0.0, 100.0).round();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let remaining = remaining as u8;
            let window_key = window.resets_at.as_deref().unwrap_or(window.id.as_str());
            notifications.push(crate::notifications::SystemNotification::low_usage(
                &item.provider,
                remaining,
                window_key,
            ));
        }
    }
    notifications
}

fn provider_is_configured(config: &AppConfig, provider: &str) -> bool {
    config.llm.preferred_provider == provider
        || config
            .llm
            .provider_order
            .iter()
            .any(|candidate| candidate == provider)
        || config
            .llm
            .providers
            .iter()
            .any(|entry| entry.id == provider && entry.enabled)
}

/// Probe programmatically reported usage for providers participating in scheduled runs.
/// Claude and Grok consumer plans intentionally remain failure-inferred because neither
/// exposes a supported headless quota contract.
pub(crate) async fn scheduled_low_usage_notifications(
    config: &AppConfig,
) -> Vec<crate::notifications::SystemNotification> {
    if !config.notifications.enabled || !config.notifications.low_usage {
        return Vec::new();
    }
    let mut health = Vec::new();
    if provider_is_configured(config, "openai") {
        health.push(provider_health(config, "openai").await);
    }
    low_usage_notifications(config, &health)
}

/// Disk-backed snapshot of the last probe per provider.
const HEALTH_CACHE_FILE: &str = "provider-health.json";

/// Bump to discard snapshots this version can no longer read, rather than migrate them.
const HEALTH_CACHE_VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct HealthCacheFile {
    version: u32,
    entries: Vec<ProviderHealth>,
}

fn health_cache() -> &'static Mutex<Option<HashMap<String, ProviderHealth>>> {
    static CACHE: OnceLock<Mutex<Option<HashMap<String, ProviderHealth>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn health_cache_path() -> std::path::PathBuf {
    super::state_dir().join(HEALTH_CACHE_FILE)
}

/// Read the persisted snapshots, or an empty map when absent or unreadable.
///
/// A corrupt or superseded cache is never an error: usage is advisory, and a
/// failed read simply means the next refresh repopulates it.
fn load_health_cache() -> HashMap<String, ProviderHealth> {
    let Ok(bytes) = std::fs::read(health_cache_path()) else {
        return HashMap::new();
    };
    let Ok(file) = serde_json::from_slice::<HealthCacheFile>(&bytes) else {
        return HashMap::new();
    };
    if file.version != HEALTH_CACHE_VERSION {
        return HashMap::new();
    }
    file.entries
        .into_iter()
        .map(|entry| (entry.provider.clone(), entry))
        .collect()
}

/// Persist snapshots with the same temp + fsync + rename used for notification history.
fn persist_health_cache(entries: &HashMap<String, ProviderHealth>) {
    let path = health_cache_path();
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let mut entries: Vec<ProviderHealth> = entries.values().cloned().collect();
    entries.sort_by(|a, b| a.provider.cmp(&b.provider));
    let file = HealthCacheFile {
        version: HEALTH_CACHE_VERSION,
        entries,
    };
    let Ok(bytes) = serde_json::to_vec(&file) else {
        return;
    };
    let mut name = path.file_name().map_or_else(
        || std::ffi::OsString::from(HEALTH_CACHE_FILE),
        std::ffi::OsStr::to_os_string,
    );
    name.push(format!(".{}.tmp", std::process::id()));
    let temp = path.with_file_name(name);
    if std::fs::write(&temp, &bytes).is_ok() {
        let _ = std::fs::rename(&temp, &path);
    }
}

/// Providers worth listing and probing: those the user actually participates with.
///
/// Listing all six meant five rows reading "Usage unavailable" — and each cost a
/// probe. Order follows `PROVIDERS` so the section is stable across refreshes.
fn health_targets(config: &AppConfig) -> Vec<&'static str> {
    let configured: Vec<&'static str> = PROVIDERS
        .iter()
        .map(|definition| definition.id)
        .filter(|id| provider_is_configured(config, id))
        .collect();
    if configured.is_empty() {
        // A user who has configured nothing still deserves to see the provider
        // a render would actually reach for.
        return PROVIDERS
            .iter()
            .map(|definition| definition.id)
            .filter(|id| *id == config.llm.preferred_provider)
            .collect();
    }
    configured
}

/// Read the last known health for every configured provider.
///
/// This is a pure cache read: it never probes a provider, never spawns a
/// process and never raises a notification. Probing lives in
/// [`refresh_provider_health`], which the UI calls explicitly. The two used to
/// be the same function, so merely opening Settings ran six sequential probes —
/// one of them an 8-second process spawn — and delivered low-usage
/// notifications as a side effect of a render.
#[tauri::command]
pub async fn get_provider_health(app_handle: AppHandle) -> Result<Vec<ProviderHealth>, AppError> {
    let config = crate::commands::load_config(&app_handle)?;
    let mut guard = health_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let cached = guard.get_or_insert_with(load_health_cache);
    Ok(health_targets(&config)
        .into_iter()
        .map(|id| {
            cached
                .get(id)
                .cloned()
                .unwrap_or_else(|| unknown_health(id, "awaiting_first_refresh"))
        })
        .collect())
}

/// Refresh one provider, or all providers when `provider` is omitted.
#[tauri::command]
pub async fn refresh_provider_health(
    app_handle: AppHandle,
    provider: Option<String>,
) -> Result<Vec<ProviderHealth>, AppError> {
    if provider
        .as_deref()
        .is_some_and(|id| binary_for(id).is_none())
    {
        return Err(AppError::Invalid(format!(
            "unknown provider: {}",
            provider.as_deref().unwrap_or_default()
        )));
    }
    let config = crate::commands::load_config(&app_handle)?;
    let ids: Vec<String> = provider.as_deref().map_or_else(
        || {
            health_targets(&config)
                .into_iter()
                .map(str::to_string)
                .collect()
        },
        |id| vec![id.to_string()],
    );

    // Probe concurrently. Sequentially, one slow provider delayed every other —
    // and the Codex probe alone is allowed eight seconds.
    let mut probes = tokio::task::JoinSet::new();
    for (index, id) in ids.iter().enumerate() {
        let config = config.clone();
        let id = id.clone();
        probes.spawn(async move { (index, provider_health(&config, &id).await) });
    }
    let mut snapshots: Vec<Option<ProviderHealth>> = vec![None; ids.len()];
    while let Some(joined) = probes.join_next().await {
        match joined {
            Ok((index, health)) => snapshots[index] = Some(health),
            Err(err) => tracing::warn!(error = %err, "provider health probe panicked"),
        }
    }
    let health: Vec<ProviderHealth> = ids
        .iter()
        .zip(snapshots)
        .map(|(id, result)| result.unwrap_or_else(|| unknown_health(id, "probe_failed")))
        .collect();

    {
        let mut guard = health_cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let cached = guard.get_or_insert_with(load_health_cache);
        for entry in &health {
            cached.insert(entry.provider.clone(), entry.clone());
        }
        persist_health_cache(cached);
    }

    for notification in low_usage_notifications(&config, &health) {
        if let Err(err) =
            crate::notifications::notify_gui(&app_handle, &config.notifications, &notification)
        {
            tracing::warn!(error = %err, "could not deliver low-usage notification");
        }
    }
    app_handle
        .emit("provider-health-updated", &health)
        .map_err(|err| AppError::Config(format!("emit provider health: {err}")))?;
    Ok(health)
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

/// Resolve the saved `api_base_url` for `provider`, or the documented default.
fn api_base_for(provider: &str, stored: &ProviderConfig) -> String {
    match provider {
        "ollama" => stored
            .api_base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string()),
        "claude" => stored
            .api_base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".to_string()),
        "openai" => stored
            .api_base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com".to_string()),
        "gemini" => stored
            .api_base_url
            .clone()
            .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string()),
        "grok" => stored
            .api_base_url
            .clone()
            .unwrap_or_else(|| "https://api.x.ai".to_string()),
        _ => String::new(),
    }
}

/// Resolve the API key for `provider` via keychain + env, returning `None` if unset.
fn resolve_key(provider: &str, stored: &ProviderConfig) -> Option<String> {
    let env_vars: &[&str] = match provider {
        "claude" => &["ANTHROPIC_API_KEY"],
        "openai" => &["OPENAI_API_KEY"],
        "gemini" => &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        "grok" => &["XAI_API_KEY", "GROK_API_KEY"],
        _ => &[],
    };
    autostand_adapters::llm::helpers::resolve_api_key(
        &crate::render::provider_config(stored, None),
        provider,
        env_vars,
    )
}

/// Parse a `/v1/models`-style response (`{ "data": [{ "id": "..." }] }`) into a
/// sorted, deduped list of model ids.
fn parse_openai_models(resp: &serde_json::Value) -> Vec<String> {
    resp.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            let mut ids: Vec<String> = arr
                .iter()
                .filter_map(|entry| entry.get("id").and_then(|id| id.as_str()).map(String::from))
                .collect();
            ids.sort();
            ids.dedup();
            ids
        })
        .unwrap_or_default()
}

/// Parse a `/v1beta/models?key=…` response (`{ "models": [{ "name": "models/foo" }] }`)
/// stripping the `models/` prefix.
fn parse_gemini_models(resp: &serde_json::Value) -> Vec<String> {
    resp.get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            let mut ids: Vec<String> = arr
                .iter()
                .filter_map(|entry| entry.get("name").and_then(|n| n.as_str()).map(String::from))
                .map(|n| n.strip_prefix("models/").unwrap_or(&n).to_string())
                .collect();
            ids.sort();
            ids.dedup();
            ids
        })
        .unwrap_or_default()
}

/// Parse an Ollama `/api/tags` response (`{ "models": [{ "name": "llama3.2:latest" }] }`).
fn parse_ollama_models(resp: &serde_json::Value) -> Vec<String> {
    resp.get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            let mut ids: Vec<String> = arr
                .iter()
                .filter_map(|entry| entry.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect();
            ids.sort();
            ids.dedup();
            ids
        })
        .unwrap_or_default()
}

/// List available models for a provider via its API.
///
/// Returns an empty `Vec` (never an error) when the API key is missing or the
/// endpoint is unreachable — the Settings UI shows the saved model only, so a
/// failed probe is not a verdict on the saved config.
#[tauri::command]
pub async fn list_provider_models(
    app_handle: AppHandle,
    provider: String,
) -> Result<Vec<String>, AppError> {
    if binary_for(&provider).is_none() {
        return Err(AppError::Invalid(format!("unknown provider: {provider}")));
    }
    if provider == "builtin-local" {
        return Ok(crate::commands::local_models::available_model_ids(
            &app_handle,
        ));
    }
    let config = crate::commands::load_config(&app_handle)?;
    let stored = stored_entry(&config, &provider);
    let base = api_base_for(&provider, &stored);
    if base.is_empty() {
        return Ok(Vec::new());
    }
    let client = autostand_adapters::llm::helpers::build_client();
    let timeout = 15u64;

    let result = match provider.as_str() {
        "ollama" => {
            let url = format!("{base}/api/tags");
            let auth = resolve_key(&provider, &stored);
            let mut headers: Vec<(&str, &str)> = Vec::new();
            if let Some(ref k) = auth {
                headers.push(("Authorization", k.as_str()));
            }
            autostand_adapters::llm::helpers::http_get_json(&client, &url, &headers, timeout)
                .await
                .map(|resp| parse_ollama_models(&resp))
        }
        "claude" => match resolve_key(&provider, &stored) {
            Some(key) => {
                let url = format!("{base}/v1/models");
                let headers = [
                    ("x-api-key", key.as_str()),
                    ("anthropic-version", "2023-06-01"),
                ];
                autostand_adapters::llm::helpers::http_get_json(&client, &url, &headers, timeout)
                    .await
                    .map(|resp| parse_openai_models(&resp))
            }
            None => Err(LlmError::AuthError),
        },
        "openai" => match resolve_key(&provider, &stored) {
            Some(key) => {
                let url = format!("{base}/v1/models");
                let auth = format!("Bearer {key}");
                let headers = [("Authorization", auth.as_str())];
                autostand_adapters::llm::helpers::http_get_json(&client, &url, &headers, timeout)
                    .await
                    .map(|resp| parse_openai_models(&resp))
            }
            None => Err(LlmError::AuthError),
        },
        "grok" => match resolve_key(&provider, &stored) {
            Some(key) => {
                let url = format!("{base}/v1/models");
                let auth = format!("Bearer {key}");
                let headers = [("Authorization", auth.as_str())];
                autostand_adapters::llm::helpers::http_get_json(&client, &url, &headers, timeout)
                    .await
                    .map(|resp| parse_openai_models(&resp))
            }
            None => Err(LlmError::AuthError),
        },
        "gemini" => match resolve_key(&provider, &stored) {
            Some(key) => {
                let url = format!("{base}/v1beta/models?key={key}");
                autostand_adapters::llm::helpers::http_get_json(&client, &url, &[], timeout)
                    .await
                    .map(|resp| parse_gemini_models(&resp))
            }
            None => Err(LlmError::AuthError),
        },
        _ => return Ok(Vec::new()),
    };

    match result {
        Ok(models) => Ok(models),
        Err(err) => {
            tracing::warn!(
                provider = %provider,
                reason = %probe_failure(&err),
                "model list probe failed"
            );
            Ok(Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        api_base_for, binary_for, env_var_for, health_targets, inferred_health, is_local_only,
        low_usage_notifications, parse_codex_health, parse_gemini_models, parse_ollama_models,
        parse_openai_models, probe_failure, probe_mode, provider_is_configured,
        record_provider_attempts, stored_entry, AdapterMode, AppConfig, LlmError, ProviderConfig,
        PROVIDERS,
    };
    use crate::commands::types::{
        ProviderAvailability, ProviderHealth, ProviderMode, UsageSource, UsageWindow,
    };
    use crate::render::{ProviderAttempt, ProviderAttemptChannel, ProviderAttemptStatus};

    #[test]
    fn exposes_all_documented_providers() {
        let ids: Vec<&str> = PROVIDERS.iter().map(|p| p.id).collect();
        assert_eq!(
            ids,
            [
                "builtin-local",
                "claude",
                "ollama",
                "openai",
                "gemini",
                "grok"
            ]
        );
    }

    #[test]
    fn maps_provider_ids_to_cli_binaries() {
        assert_eq!(binary_for("claude"), Some("claude"));
        assert_eq!(binary_for("openai"), Some("codex"));
        assert_eq!(binary_for("builtin-local"), Some("autostand-local-llm"));
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
    fn built_in_and_ollama_are_local_only() {
        assert!(is_local_only("ollama"));
        assert!(is_local_only("builtin-local"));
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
        assert_eq!(env_var_for("builtin-local"), None);
        assert_eq!(env_var_for("nope"), None);
    }

    #[test]
    fn scheduled_usage_probe_only_runs_for_configured_codex() {
        let mut config = AppConfig::default();
        assert!(!provider_is_configured(&config, "openai"));
        config.llm.provider_order = vec!["grok".to_string(), "openai".to_string()];
        assert!(provider_is_configured(&config, "openai"));
    }

    #[test]
    fn low_usage_notifications_respect_threshold_without_inventing_unknown_values() {
        let mut config = AppConfig::default();
        config.notifications.low_usage_threshold_percent = 20;
        let known = ProviderHealth {
            provider: "openai".to_string(),
            availability: ProviderAvailability::Low,
            source: UsageSource::ProviderReported,
            windows: vec![UsageWindow {
                id: "weekly".to_string(),
                used_percent: Some(85.0),
                remaining_percent: Some(15.0),
                resets_at: Some("2030-01-01T00:00:00Z".to_string()),
            }],
            reason: None,
            checked_at: "2026-08-13T00:00:00Z".to_string(),
        };
        let unknown = ProviderHealth {
            provider: "grok".to_string(),
            availability: ProviderAvailability::Unknown,
            source: UsageSource::Unknown,
            windows: Vec::new(),
            reason: Some("usage_not_programmatically_available".to_string()),
            checked_at: "2026-08-13T00:00:00Z".to_string(),
        };

        let notifications = low_usage_notifications(&config, &[known, unknown]);
        assert_eq!(notifications.len(), 1);
        assert!(notifications[0].body.contains("15% remaining"));
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
    fn parse_openai_models_sorts_and_dedups_ids() {
        let resp = serde_json::json!({
            "data": [
                { "id": "gpt-4o" },
                { "id": "o3" },
                { "id": "gpt-4o" },
                { "object": "model" }
            ]
        });
        assert_eq!(parse_openai_models(&resp), ["gpt-4o", "o3"]);
    }

    #[test]
    fn parse_openai_models_is_empty_when_the_payload_is_malformed() {
        assert!(parse_openai_models(&serde_json::json!({})).is_empty());
        assert!(parse_openai_models(&serde_json::json!({ "data": "nope" })).is_empty());
    }

    #[test]
    fn parse_gemini_models_strips_the_models_prefix() {
        let resp = serde_json::json!({
            "models": [
                { "name": "models/gemini-2.5-pro" },
                { "name": "gemini-2.0-flash" },
                { "name": "models/gemini-2.5-pro" }
            ]
        });
        assert_eq!(
            parse_gemini_models(&resp),
            ["gemini-2.0-flash", "gemini-2.5-pro"]
        );
    }

    #[test]
    fn parse_ollama_models_reads_local_tag_names() {
        let resp = serde_json::json!({
            "models": [
                { "name": "llama3.2:latest" },
                { "name": "mistral" },
                { "digest": "sha256:abc" }
            ]
        });
        assert_eq!(parse_ollama_models(&resp), ["llama3.2:latest", "mistral"]);
    }

    #[test]
    fn api_base_for_uses_the_documented_default_or_the_saved_override() {
        let blank = ProviderConfig::default();
        assert_eq!(api_base_for("ollama", &blank), "http://localhost:11434");
        assert_eq!(api_base_for("claude", &blank), "https://api.anthropic.com");
        assert_eq!(api_base_for("openai", &blank), "https://api.openai.com");
        assert_eq!(
            api_base_for("gemini", &blank),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(api_base_for("grok", &blank), "https://api.x.ai");
        assert_eq!(api_base_for("nope", &blank), "");

        let override_cfg = ProviderConfig {
            api_base_url: Some("http://proxy.local".to_string()),
            ..ProviderConfig::default()
        };
        assert_eq!(api_base_for("claude", &override_cfg), "http://proxy.local");
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

    #[test]
    fn codex_health_prefers_the_named_codex_limit_and_maps_windows() {
        let result = serde_json::json!({
            "rateLimits": {
                "primary": { "usedPercent": 99.0, "windowDurationMins": 300 }
            },
            "rateLimitsByLimitId": {
                "codex": {
                    "primary": {
                        "usedPercent": 20.0,
                        "windowDurationMins": 10080,
                        "resetsAt": 1_786_560_000_i64
                    },
                    "secondary": null
                }
            }
        });
        let health = parse_codex_health(&result).expect("Codex rate limit");
        assert_eq!(health.source, UsageSource::ProviderReported);
        assert_eq!(health.availability, ProviderAvailability::Available);
        assert_eq!(health.windows.len(), 1);
        assert_eq!(health.windows[0].id, "weekly");
        assert_eq!(health.windows[0].used_percent, Some(20.0));
        assert_eq!(health.windows[0].remaining_percent, Some(80.0));
        assert!(health.windows[0].resets_at.is_some());
    }

    #[test]
    fn codex_health_marks_a_low_or_exhausted_window_without_inventing_values() {
        for (used, expected) in [
            (80.0, ProviderAvailability::Low),
            (100.0, ProviderAvailability::Exhausted),
        ] {
            let result = serde_json::json!({
                "rateLimits": {
                    "primary": { "usedPercent": used, "windowDurationMins": 300 }
                }
            });
            let health = parse_codex_health(&result).expect("Codex rate limit");
            assert_eq!(health.availability, expected);
            assert_eq!(health.windows[0].id, "five_hour");
        }
        assert!(parse_codex_health(&serde_json::json!({})).is_none());
    }

    #[test]
    fn exhausted_failures_are_exposed_as_inferred_health_without_a_fake_percentage() {
        record_provider_attempts(&[ProviderAttempt {
            provider: "grok".to_string(),
            channel: Some(ProviderAttemptChannel::Cli),
            model: "grok-4".to_string(),
            status: ProviderAttemptStatus::Failed,
            reason: Some("usage_balance_exhausted".to_string()),
            latency_ms: None,
        }]);
        let health = inferred_health()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove("grok")
            .expect("inferred Grok health");
        assert_eq!(health.availability, ProviderAvailability::Exhausted);
        assert_eq!(health.source, UsageSource::FailureInferred);
        assert!(health.windows.is_empty());
        assert_eq!(health.reason.as_deref(), Some("usage_balance_exhausted"));
    }

    /// Listing all six providers meant five rows reading "Usage unavailable",
    /// and each one still cost a probe.
    #[test]
    fn health_targets_are_limited_to_configured_providers() {
        let mut config = AppConfig::default();
        config.llm.preferred_provider = "openai".to_string();
        config.llm.provider_order = vec!["openai".to_string(), "claude".to_string()];
        config.llm.providers = Vec::new();

        let targets = health_targets(&config);
        assert!(targets.contains(&"openai"));
        assert!(targets.contains(&"claude"));
        assert!(!targets.contains(&"gemini"));
        assert!(!targets.contains(&"ollama"));
    }

    /// Targets follow `PROVIDERS` order, so the section does not reshuffle
    /// between refreshes.
    #[test]
    fn health_targets_follow_the_catalogue_order() {
        let mut config = AppConfig::default();
        config.llm.preferred_provider = "grok".to_string();
        config.llm.provider_order = vec!["openai".to_string(), "claude".to_string()];
        config.llm.providers = Vec::new();

        let targets = health_targets(&config);
        let catalogue: Vec<&str> = PROVIDERS
            .iter()
            .map(|definition| definition.id)
            .filter(|id| targets.contains(id))
            .collect();
        assert_eq!(targets, catalogue);
    }

    /// A user who has configured nothing still sees the provider a render would
    /// reach for, rather than an empty section.
    #[test]
    fn health_targets_fall_back_to_the_preferred_provider() {
        let mut config = AppConfig::default();
        config.llm.preferred_provider = "gemini".to_string();
        config.llm.provider_order = Vec::new();
        config.llm.providers = Vec::new();

        assert_eq!(health_targets(&config), vec!["gemini"]);
    }
}
