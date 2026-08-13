//! `Grok` (`xAI`) adapter — `Grok` `CLI` (auto-detect variant) + `xAI` `API`.
//! See `docs/llm-adapters/05-grok.md`.

use super::helpers;
use super::{
    detect_cli_binary, CliInfo, LlmAdapter, LlmError, ProviderConfig, ProviderMode, RenderModeUsed,
    RenderOutput, TestResult,
};
use async_trait::async_trait;
use serde_json::json;
use std::time::Instant;

/// Grok CLI variant fingerprinted by `--version` output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GrokVariant {
    /// Auto-detect (default).
    #[default]
    Auto,
    /// Official xAI CLI ("Grok Build").
    Official,
    /// `superagent-ai/grok-cli`.
    Superagent,
    /// `grokcli.dev`.
    GrokCliDev,
}

/// Grok adapter.
#[derive(Debug, Default)]
pub struct GrokAdapter {
    cli_path: Option<std::path::PathBuf>,
}

impl GrokAdapter {
    fn api_base(config: &ProviderConfig) -> String {
        config
            .api_base_url
            .clone()
            .unwrap_or_else(|| "https://api.x.ai".to_string())
    }

    async fn detect_cmd(&self) -> Option<(String, GrokVariant)> {
        for name in ["grok", "grok-build", "grok-cli"] {
            if let Some(info) = detect_cli_binary(name).await {
                let variant = fingerprint(&info.version);
                return Some((info.path.to_string_lossy().to_string(), variant));
            }
        }
        None
    }

    async fn render_cli(
        &self,
        prompt: &str,
        system_prompt: &str,
        config: &ProviderConfig,
    ) -> Result<RenderOutput, LlmError> {
        let (cmd, variant) = match config.cli_path.as_ref() {
            Some(p) if !p.as_os_str().is_empty() => {
                let path = p.to_string_lossy().to_string();
                let variant = match super::detect::detect_cli_at(p).await {
                    Some(info) => fingerprint(&info.version),
                    None => GrokVariant::Official,
                };
                (path, variant)
            }
            _ => self
                .detect_cmd()
                .await
                .ok_or_else(|| LlmError::CliNotFound {
                    searched: vec!["grok".into(), "grok-build".into(), "grok-cli".into()],
                })?,
        };
        let model = config.model.clone();
        let combined = if system_prompt.is_empty() {
            prompt.to_string()
        } else {
            format!("{system_prompt}\n\n{prompt}")
        };
        let start = Instant::now();
        let body = match variant {
            GrokVariant::Official | GrokVariant::Auto => {
                // Official Grok Build is a TUI. A bare `grok "<prompt>"` opens
                // an interactive session and hangs until timeout. Headless
                // mode is `--prompt-file` (single-turn) + `--output-format
                // plain`. The prompt goes in a temp file so a 50 KB standup
                // request cannot blow ARG_MAX.
                run_official_headless(&cmd, &combined, &model, config.timeout_secs.max(1)).await?
            }
            GrokVariant::Superagent | GrokVariant::GrokCliDev => {
                helpers::run_cli(&cmd, &[&combined], "", config.timeout_secs.max(1), &[]).await?
            }
        };
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
        let key = helpers::resolve_api_key(config, "grok", &["XAI_API_KEY", "GROK_API_KEY"])
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
        let (cmd, variant) = if let Some(p) = config
            .cli_path
            .as_ref()
            .filter(|p| !p.as_os_str().is_empty())
        {
            let v = match super::detect::detect_cli_at(p).await {
                Some(info) => fingerprint(&info.version),
                None => GrokVariant::Auto,
            };
            (p.to_string_lossy().to_string(), v)
        } else {
            self.detect_cmd()
                .await
                .ok_or_else(|| LlmError::CliNotFound {
                    searched: vec!["grok".into(), "grok-cli".into()],
                })?
        };
        let start = Instant::now();
        let out = helpers::run_cli(&cmd, &["--version"], "", 15, &[]).await?;
        Ok(TestResult {
            ok: true,
            message: format!("{out} ({variant:?})"),
            latency_ms: helpers::latency_ms(start),
        })
    }

    async fn test_api(&self, config: &ProviderConfig) -> Result<TestResult, LlmError> {
        let key = helpers::resolve_api_key(config, "grok", &["XAI_API_KEY", "GROK_API_KEY"])
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
            message: format!("xAI API reachable ({n} models)"),
            latency_ms: helpers::latency_ms(start),
        })
    }
}

/// Headless argv for the official Grok Build TUI.
///
/// A positional prompt starts the interactive UI; `--prompt-file` is the
/// documented single-turn path that prints to stdout and exits.
pub(crate) fn official_headless_args<'a>(prompt_file: &'a str, model: &'a str) -> Vec<&'a str> {
    let mut args = vec![
        "--prompt-file",
        prompt_file,
        "--output-format",
        "plain",
        "--verbatim",
        "--max-turns",
        "1",
        "--permission-mode",
        "dontAsk",
        "--no-memory",
        "--no-subagents",
        "--disable-web-search",
    ];
    if !model.trim().is_empty() {
        args.extend(["-m", model]);
    }
    args
}

async fn run_official_headless(
    cmd: &str,
    prompt: &str,
    model: &str,
    timeout_secs: u64,
) -> Result<String, LlmError> {
    let path = std::env::temp_dir().join(format!(
        "autostand-grok-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    std::fs::write(&path, prompt).map_err(|err| LlmError::CliExitError {
        code: -1,
        stderr: format!("could not write grok prompt file: {err}"),
    })?;
    let file = path.to_string_lossy().into_owned();
    let args = official_headless_args(&file, model);
    let result = helpers::run_cli(cmd, &args, "", timeout_secs, &[]).await;
    let _ = std::fs::remove_file(&path);
    result
}

fn fingerprint(version: &str) -> GrokVariant {
    let v = version.to_lowercase();
    if v.contains("superagent") || v.contains("super-agent") {
        GrokVariant::Superagent
    } else if v.contains("grokcli.dev") || v.contains("grok-cli.dev") {
        GrokVariant::GrokCliDev
    } else {
        GrokVariant::Official
    }
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
        if let Some(p) = &self.cli_path {
            if !p.as_os_str().is_empty() {
                return super::detect::detect_cli_at(p).await;
            }
        }
        if let Some(info) = detect_cli_binary("grok").await {
            return Some(info);
        }
        if let Some(info) = detect_cli_binary("grok-build").await {
            return Some(info);
        }
        detect_cli_binary("grok-cli").await
    }

    async fn has_api_key(&self) -> bool {
        helpers::load_api_key("grok", &["XAI_API_KEY", "GROK_API_KEY"]).is_some()
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

#[cfg(test)]
mod tests {
    use super::official_headless_args;

    #[test]
    fn official_headless_args_are_single_turn_not_a_tui_prompt() {
        let args = official_headless_args("/tmp/prompt.txt", "grok-4.5");
        assert!(args.contains(&"--prompt-file"));
        assert!(args.contains(&"/tmp/prompt.txt"));
        assert!(args.contains(&"--output-format"));
        assert!(args.contains(&"plain"));
        assert!(args.contains(&"--verbatim"));
        assert!(args.contains(&"--max-turns"));
        assert!(args.contains(&"1"));
        assert!(args.contains(&"--no-memory"));
        assert!(args.contains(&"--no-subagents"));
        assert!(args.contains(&"-m"));
        assert!(args.contains(&"grok-4.5"));
        // A lone positional string is what opens the interactive TUI.
        assert!(!args.iter().any(|a| a.starts_with("You are a daily")));
    }

    #[test]
    fn official_headless_args_omit_blank_model() {
        let args = official_headless_args("/tmp/p.txt", "");
        assert!(!args.contains(&"-m"));
    }
}
