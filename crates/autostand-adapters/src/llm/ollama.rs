//! `Ollama` adapter — `CLI` `ollama run` + local `API`.
//! See `docs/llm-adapters/02-ollama.md`.

use super::helpers;
use super::{
    detect_cli_binary, CliInfo, LlmAdapter, LlmError, ProviderConfig, ProviderMode, RenderModeUsed,
    RenderOutput, TestResult,
};
use async_trait::async_trait;
use serde_json::json;
use std::time::Instant;

/// Ollama adapter.
#[derive(Debug, Default)]
pub struct OllamaAdapter {
    #[allow(dead_code)]
    cli_path: Option<std::path::PathBuf>,
}

impl OllamaAdapter {
    fn api_base(config: &ProviderConfig) -> String {
        config
            .api_base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string())
    }

    async fn render_cli(
        &self,
        prompt: &str,
        system_prompt: &str,
        config: &ProviderConfig,
    ) -> Result<RenderOutput, LlmError> {
        let cmd = helpers::resolve_cli_cmd(config, "ollama")
            .await
            .ok_or_else(|| LlmError::CliNotFound {
                searched: vec!["ollama".into()],
            })?;
        let model = config.model.clone();
        let combined = if system_prompt.is_empty() {
            prompt.to_string()
        } else {
            format!("{system_prompt}\n\n{prompt}")
        };
        let args: Vec<&str> = vec!["run", &model];
        let start = Instant::now();
        let body =
            helpers::run_cli(&cmd, &args, &combined, config.timeout_secs.max(1), &[]).await?;
        Ok(RenderOutput {
            body,
            mode_used: RenderModeUsed::Cli,
            model,
            latency_ms: helpers::latency_ms(start),
        })
    }

    async fn render_api(
        &self,
        prompt: &str,
        system_prompt: &str,
        config: &ProviderConfig,
    ) -> Result<RenderOutput, LlmError> {
        let model = config.model.clone();
        let url = format!("{}/api/chat", Self::api_base(config));
        let body = json!({
            "model": model,
            "stream": false,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": prompt },
            ],
        });
        let remote_key = helpers::resolve_api_key(config, "ollama", &[]);
        let auth_value = remote_key.clone().unwrap_or_default();
        let mut headers: Vec<(&str, &str)> = vec![("content-type", "application/json")];
        if remote_key.is_some() {
            headers.push(("Authorization", auth_value.as_str()));
        }
        let client = helpers::build_client();
        let start = Instant::now();
        let resp =
            helpers::http_post_json(&client, &url, &headers, &body, config.timeout_secs.max(1))
                .await?;
        let text = resp
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| LlmError::ParseError {
                raw: "missing message.content".into(),
            })?
            .to_string();
        Ok(RenderOutput {
            body: text,
            mode_used: RenderModeUsed::Api,
            model,
            latency_ms: helpers::latency_ms(start),
        })
    }

    async fn test_cli(&self, config: &ProviderConfig) -> Result<TestResult, LlmError> {
        let cmd = helpers::resolve_cli_cmd(config, "ollama")
            .await
            .ok_or_else(|| LlmError::CliNotFound {
                searched: vec!["ollama".into()],
            })?;
        let start = Instant::now();
        let out = helpers::run_cli_probe(&cmd, &["--version"], 15).await?;
        Ok(TestResult {
            ok: true,
            message: out,
            latency_ms: helpers::latency_ms(start),
        })
    }

    async fn test_api(&self, config: &ProviderConfig) -> Result<TestResult, LlmError> {
        let url = format!("{}/api/tags", Self::api_base(config));
        let remote_key = helpers::resolve_api_key(config, "ollama", &[]);
        let auth_value = remote_key.clone().unwrap_or_default();
        let mut headers: Vec<(&str, &str)> = Vec::new();
        if remote_key.is_some() {
            headers.push(("Authorization", auth_value.as_str()));
        }
        let client = helpers::build_client();
        let start = Instant::now();
        let resp = helpers::http_get_json(&client, &url, &headers, 15).await?;
        let n = resp
            .get("models")
            .and_then(|m| m.as_array())
            .map_or(0, Vec::len);
        Ok(TestResult {
            ok: true,
            message: format!("Ollama reachable ({n} models)"),
            latency_ms: helpers::latency_ms(start),
        })
    }
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
        true
    }

    async fn render(
        &self,
        prompt: &str,
        system_prompt: &str,
        config: &ProviderConfig,
    ) -> Result<RenderOutput, LlmError> {
        match config.mode {
            ProviderMode::CliOnly => self.render_cli(prompt, system_prompt, config).await,
            ProviderMode::ApiOnly => self.render_api(prompt, system_prompt, config).await,
            ProviderMode::CliFirst => match self.render_cli(prompt, system_prompt, config).await {
                Ok(out) => Ok(out),
                Err(LlmError::CliNotFound { .. }) => {
                    self.render_api(prompt, system_prompt, config).await
                }
                Err(e) => Err(e),
            },
            ProviderMode::ApiFallback => match self.render_api(prompt, system_prompt, config).await
            {
                Ok(out) => Ok(out),
                Err(LlmError::ApiError { .. }) => {
                    self.render_cli(prompt, system_prompt, config).await
                }
                Err(e) => Err(e),
            },
        }
    }

    async fn test_connection(&self, config: &ProviderConfig) -> Result<TestResult, LlmError> {
        match config.mode {
            ProviderMode::CliOnly => self.test_cli(config).await,
            ProviderMode::ApiOnly => self.test_api(config).await,
            ProviderMode::CliFirst => match self.test_cli(config).await {
                Ok(r) => Ok(r),
                Err(_) => self.test_api(config).await,
            },
            ProviderMode::ApiFallback => match self.test_api(config).await {
                Ok(r) => Ok(r),
                Err(_) => self.test_cli(config).await,
            },
        }
    }
}
