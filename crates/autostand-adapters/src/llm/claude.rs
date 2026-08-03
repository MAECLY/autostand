//! `Claude` (`Anthropic`) adapter — `CLI` `claude -p` + `API`.
//! See `docs/llm-adapters/01-claude.md`.

use super::{
    detect_cli_binary, CliInfo, LlmAdapter, LlmError, ProviderConfig, RenderOutput, TestResult,
};
use async_trait::async_trait;

/// Claude adapter.
#[derive(Debug, Default)]
pub struct ClaudeAdapter {
    #[allow(dead_code)]
    cli_path: Option<std::path::PathBuf>,
}

#[async_trait]
impl LlmAdapter for ClaudeAdapter {
    fn id(&self) -> &'static str {
        "claude"
    }
    fn display_name(&self) -> &'static str {
        "Claude (Anthropic)"
    }

    async fn detect_cli(&self) -> Option<CliInfo> {
        detect_cli_binary("claude").await
    }

    async fn has_api_key(&self) -> bool {
        keyring::Entry::new("autostand", "claude")
            .and_then(|e| e.get_password())
            .is_ok()
    }

    async fn render(
        &self,
        _prompt: &str,
        _system_prompt: &str,
        _config: &ProviderConfig,
    ) -> Result<RenderOutput, LlmError> {
        // TODO: CLI `claude -p --model <model> "<prompt>"` with AUTOSTAND_RENDER=1 env
        Err(LlmError::ParseError {
            raw: "not implemented".into(),
        })
    }

    async fn test_connection(&self, _config: &ProviderConfig) -> Result<TestResult, LlmError> {
        Ok(TestResult {
            ok: false,
            message: "not implemented".into(),
            latency_ms: 0,
        })
    }
}
