//! `Claude` (`Anthropic`) adapter — `CLI` `claude -p` + `API`.
//! See `docs/llm-adapters/01-claude.md`.

use super::helpers;
use super::{
    detect_cli_binary, CliInfo, LlmAdapter, LlmError, ProviderConfig, ProviderMode, RenderModeUsed,
    RenderOutput, TestResult,
};
use async_trait::async_trait;
use serde_json::json;
use std::time::Instant;

/// Claude adapter.
#[derive(Debug, Default)]
pub struct ClaudeAdapter {
    cli_path: Option<std::path::PathBuf>,
}

impl ClaudeAdapter {
    fn resolve_model(setting: &str) -> &str {
        match setting {
            "haiku" => "claude-haiku-4-5",
            "opus" => "claude-opus-4-1",
            _ => "claude-sonnet-4-5-20250929",
        }
    }

    fn api_base(config: &ProviderConfig) -> String {
        config
            .api_base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".to_string())
    }

    async fn render_cli(
        &self,
        prompt: &str,
        system_prompt: &str,
        config: &ProviderConfig,
    ) -> Result<RenderOutput, LlmError> {
        let cmd = helpers::resolve_cli_cmd(config, "claude")
            .await
            .ok_or_else(|| LlmError::CliNotFound {
                searched: vec!["claude".into()],
            })?;
        let model = config.model.clone();
        let combined_prompt = if system_prompt.is_empty() {
            prompt.to_string()
        } else {
            format!("{system_prompt}\n\n{prompt}")
        };
        // Print mode reads stdin when no positional prompt is provided. This
        // avoids ARG_MAX for large fact sets, and the render session must not
        // be persisted and gathered into tomorrow's standup.
        let args: Vec<&str> = vec!["-p", "--no-session-persistence", "--model", &model];
        let start = Instant::now();
        let body = helpers::run_cli(
            &cmd,
            &args,
            &combined_prompt,
            config.timeout_secs.max(1),
            &[("CLAUDE_STANDUP_RENDER", "1")],
        )
        .await?;
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
        let key = helpers::resolve_api_key(config, "claude", &["ANTHROPIC_API_KEY"])
            .ok_or(LlmError::AuthError)?;
        let model = Self::resolve_model(&config.model).to_string();
        let url = format!("{}/v1/messages", Self::api_base(config));
        let body = json!({
            "model": model,
            "max_tokens": 4096,
            "system": system_prompt,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let headers = [
            ("x-api-key", key.as_str()),
            ("anthropic-version", "2023-06-01"),
            ("content-type", "application/json"),
        ];
        let client = helpers::build_client();
        let start = Instant::now();
        let resp =
            helpers::http_post_json(&client, &url, &headers, &body, config.timeout_secs.max(1))
                .await?;
        let text = resp
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| LlmError::ParseError {
                raw: "missing content[0].text".into(),
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
        let cmd = helpers::resolve_cli_cmd(config, "claude")
            .await
            .ok_or_else(|| LlmError::CliNotFound {
                searched: vec!["claude".into()],
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
        let key = helpers::resolve_api_key(config, "claude", &["ANTHROPIC_API_KEY"])
            .ok_or(LlmError::AuthError)?;
        let url = format!("{}/v1/models", Self::api_base(config));
        let headers = [
            ("x-api-key", key.as_str()),
            ("anthropic-version", "2023-06-01"),
        ];
        let client = helpers::build_client();
        let start = Instant::now();
        let resp = helpers::http_get_json(&client, &url, &headers, 15).await?;
        let n = resp
            .get("data")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        Ok(TestResult {
            ok: true,
            message: format!("Anthropic API reachable ({n} models)"),
            latency_ms: helpers::latency_ms(start),
        })
    }
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
        if let Some(p) = &self.cli_path {
            if !p.as_os_str().is_empty() {
                return super::detect::detect_cli_at(p).await;
            }
        }
        detect_cli_binary("claude").await
    }

    async fn has_api_key(&self) -> bool {
        helpers::load_api_key("claude", &["ANTHROPIC_API_KEY"]).is_some()
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
