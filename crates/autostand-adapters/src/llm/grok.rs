//! `Grok` (`xAI`) adapter — `Grok` `CLI` (auto-detect variant) + `xAI` `API`.
//! See `docs/llm-adapters/05-grok.md`.

use super::{
    detect_cli_binary, CliInfo, LlmAdapter, LlmError, ProviderConfig, RenderOutput, TestResult,
};
use async_trait::async_trait;

#[derive(Debug, Default)]
pub struct GrokAdapter {
    #[allow(dead_code)]
    cli_path: Option<std::path::PathBuf>,
}

#[async_trait]
impl LlmAdapter for GrokAdapter {
    fn id(&self) -> &'static str {
        "grok"
    }
    fn display_name(&self) -> &'static str {
        "Grok (xAI)"
    }

    async fn detect_cli(&self) -> Option<CliInfo> {
        detect_cli_binary("grok").await
    }

    async fn has_api_key(&self) -> bool {
        keyring::Entry::new("autostand", "grok")
            .and_then(|e| e.get_password())
            .is_ok()
    }

    async fn render(
        &self,
        _prompt: &str,
        _system_prompt: &str,
        _config: &ProviderConfig,
    ) -> Result<RenderOutput, LlmError> {
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
