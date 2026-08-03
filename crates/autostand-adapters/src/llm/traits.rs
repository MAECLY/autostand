//! `LlmAdapter` trait + shared types.
//!
//! See `docs/llm-adapters/adapter-trait.md`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Provider access mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderMode {
    /// Try CLI local first, fall back to API key.
    CliFirst,
    /// CLI only (no API key).
    CliOnly,
    /// API only (no CLI).
    ApiOnly,
    /// API fallback (used only when CLI unavailable).
    ApiFallback,
}

/// Detected CLI binary info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliInfo {
    pub path: PathBuf,
    pub version: String,
}

/// Output of a render call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderOutput {
    pub body: String,
    pub mode_used: RenderModeUsed,
    pub model: String,
    pub latency_ms: u64,
}

/// Which mode was actually used for the render.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RenderModeUsed {
    Cli,
    Api,
}

/// LLM adapter errors.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum LlmError {
    #[error("timeout after {secs}s")]
    Timeout { secs: u64 },
    #[error("CLI not found (searched: {searched:?})")]
    CliNotFound { searched: Vec<PathBuf> },
    #[error("CLI exited with code {code}: {stderr}")]
    CliExitError { code: i32, stderr: String },
    #[error("API error (status {status}): {body}")]
    ApiError { status: u16, body: String },
    #[error("auth error")]
    AuthError,
    #[error("parse error: {raw}")]
    ParseError { raw: String },
    #[error("rate limit (retry after {retry_after_secs:?}s)")]
    RateLimit { retry_after_secs: Option<u64> },
}

/// Connection test result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
    pub latency_ms: u64,
}

/// Provider config (subset relevant to the adapter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub mode: ProviderMode,
    pub model: String,
    pub cli_path: Option<PathBuf>,
    pub api_key: Option<String>,
    pub api_base_url: Option<String>,
    pub timeout_secs: u64,
}

/// The trait every LLM provider implements.
#[async_trait]
pub trait LlmAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;

    /// Detect the CLI binary on PATH (or configured override).
    async fn detect_cli(&self) -> Option<CliInfo>;

    /// Check whether an API key is available (keychain / env).
    async fn has_api_key(&self) -> bool;

    /// Render a standup body from the prompt + system prompt.
    async fn render(
        &self,
        prompt: &str,
        system_prompt: &str,
        config: &ProviderConfig,
    ) -> Result<RenderOutput, LlmError>;

    /// Test the connection (CLI or API depending on mode).
    async fn test_connection(&self, config: &ProviderConfig) -> Result<TestResult, LlmError>;
}
