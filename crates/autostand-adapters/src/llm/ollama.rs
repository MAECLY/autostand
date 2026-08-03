//! `Ollama` adapter — `CLI` `ollama run` + local `API`.
//! See `docs/llm-adapters/02-ollama.md`.

use super::{
    detect_cli_binary, CliInfo, LlmAdapter, LlmError, ProviderConfig, RenderOutput, TestResult,
};
use async_trait::async_trait;

#[derive(Debug, Default)]
pub struct OllamaAdapter {
    #[allow(dead_code)]
    cli_path: Option<std::path::PathBuf>,
}

#[async_trait]
impl LlmAdapter for OllamaAdapter {
    fn id(&self) -> &'static str {
        "ollama"
    }
    fn display_name(&self) -> &'static str {
        "Ollama (local)"
    }

    async fn detect_cli(&self) -> Option<CliInfo> {
        detect_cli_binary("ollama").await
    }

    async fn has_api_key(&self) -> bool {
        // Local Ollama needs no key. Remote (Ollama Cloud) is optional.
        false
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
