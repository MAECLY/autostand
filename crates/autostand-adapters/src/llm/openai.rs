//! `OpenAI` / `Codex` adapter — `Codex CLI` + `OpenAI API`.
//! See `docs/llm-adapters/03-openai-codex.md`.

use super::helpers;
use super::{
    detect_cli_binary, CliInfo, LlmAdapter, LlmError, ProviderConfig, ProviderMode, RenderModeUsed,
    RenderOutput, TestResult,
};
use async_trait::async_trait;
use serde_json::json;
use std::time::Instant;

/// `OpenAI` / `Codex` adapter.
#[derive(Debug, Default)]
pub struct OpenAiAdapter {
    #[allow(dead_code)]
    cli_path: Option<std::path::PathBuf>,
}

impl OpenAiAdapter {
    fn api_base(config: &ProviderConfig) -> String {
        config
            .api_base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com".to_string())
    }

    async fn render_cli(
        &self,
        prompt: &str,
        system_prompt: &str,
        config: &ProviderConfig,
    ) -> Result<RenderOutput, LlmError> {
        let cmd = helpers::resolve_cli_cmd(config, "codex")
            .await
            .ok_or_else(|| LlmError::CliNotFound {
                searched: vec!["codex".into()],
            })?;
        let model = config.model.clone();
        let combined = if system_prompt.is_empty() {
            prompt.to_string()
        } else {
            format!("{system_prompt}\n\n{prompt}")
        };
        let args: Vec<&str> = vec!["exec", "--model", &model, &combined];
        let start = Instant::now();
        let body = helpers::run_cli(&cmd, &args, "", config.timeout_secs.max(1), &[]).await?;
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
        let key = helpers::resolve_api_key(config, "openai", &["OPENAI_API_KEY"])
            .ok_or(LlmError::AuthError)?;
        let model = config.model.clone();
        let url = format!("{}/v1/chat/completions", Self::api_base(config));
        let body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": prompt },
            ],
            "max_tokens": 4096,
        });
        let auth = format!("Bearer {key}");
        let headers = [
            ("Authorization", auth.as_str()),
            ("content-type", "application/json"),
        ];
        let client = helpers::build_client();
        let start = Instant::now();
        let resp =
            helpers::http_post_json(&client, &url, &headers, &body, config.timeout_secs.max(1))
                .await?;
        let text = resp
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| LlmError::ParseError {
                raw: "missing choices[0].message.content".into(),
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
        let cmd = helpers::resolve_cli_cmd(config, "codex")
            .await
            .ok_or_else(|| LlmError::CliNotFound {
                searched: vec!["codex".into()],
            })?;
        let start = Instant::now();
        let out = helpers::run_cli(&cmd, &["--version"], "", 15, &[]).await?;
        Ok(TestResult {
            ok: true,
            message: out,
            latency_ms: helpers::latency_ms(start),
        })
    }

    async fn test_api(&self, config: &ProviderConfig) -> Result<TestResult, LlmError> {
        let key = helpers::resolve_api_key(config, "openai", &["OPENAI_API_KEY"])
            .ok_or(LlmError::AuthError)?;
        let url = format!("{}/v1/models", Self::api_base(config));
        let auth = format!("Bearer {key}");
        let headers = [("Authorization", auth.as_str())];
        let client = helpers::build_client();
        let start = Instant::now();
        let resp = helpers::http_get_json(&client, &url, &headers, 15).await?;
        let n = resp
            .get("data")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        Ok(TestResult {
            ok: true,
            message: format!("OpenAI API reachable ({n} models)"),
            latency_ms: helpers::latency_ms(start),
        })
    }
}

#[async_trait]
impl LlmAdapter for OpenAiAdapter {
    fn id(&self) -> &'static str {
        "openai"
    }
    fn display_name(&self) -> &'static str {
        "OpenAI / Codex"
    }

    async fn detect_cli(&self) -> Option<CliInfo> {
        detect_cli_binary("codex").await
    }

    async fn has_api_key(&self) -> bool {
        helpers::load_api_key("openai", &["OPENAI_API_KEY"]).is_some()
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
                Err(LlmError::AuthError) => self.render_cli(prompt, system_prompt, config).await,
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
