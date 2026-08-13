//! Built-in local GGUF adapter using the process-isolated JSONL sidecar.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use super::{
    CliInfo, LlmAdapter, LlmError, LocalRuntimePolicy, ProviderConfig, RenderModeUsed,
    RenderOutput, TestResult,
};

/// Stable provider id used in configuration and fallback telemetry.
pub const PROVIDER_ID: &str = "builtin-local";
const SIDECAR_BINARY: &str = "autostand-local-llm";

/// Local provider. It never reads API keys or opens a listening socket.
#[derive(Debug, Default)]
pub struct BuiltinLocalAdapter;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)] // Ping remains part of protocol v1 for runtime diagnostics.
enum SidecarRequest<'a> {
    Ping {
        request_id: &'a str,
    },
    Generate {
        request_id: &'a str,
        model_path: &'a Path,
        prompt: &'a str,
        context_length: u32,
        max_tokens: u32,
        temperature: f32,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_cache_path: Option<&'a Path>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SidecarResponse {
    Ready {
        protocol_version: u32,
    },
    Pong {
        #[serde(rename = "request_id")]
        _request_id: String,
    },
    Result {
        request_id: String,
        body: String,
    },
    Error {
        #[serde(rename = "request_id")]
        _request_id: String,
        code: String,
        #[serde(rename = "message")]
        _message: String,
    },
}

fn state_root() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::config_dir)
        .map_or_else(|| PathBuf::from(".autostand"), |dir| dir.join("autostand"))
}

fn model_file(model_id: &str) -> Option<&'static str> {
    match model_id {
        "gemma3:1b" => Some("gemma-3-1b-it-Q8_0.gguf"),
        "qwen3.5:2b" => Some("Qwen3.5-2B-Q4_K_M.gguf"),
        "gemma3:4b" => Some("gemma-3-4b-it-Q4_K_M.gguf"),
        "qwen3.5:4b" => Some("Qwen3.5-4B-Q4_K_M.gguf"),
        _ => None,
    }
}

fn selected_model_id() -> Option<String> {
    #[derive(Deserialize)]
    struct State {
        selected_model: Option<String>,
    }
    std::fs::read(state_root().join("models/local/local-models.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<State>(&bytes).ok())
        .and_then(|state| state.selected_model)
}

fn resolve_model(configured: &str) -> Result<(String, PathBuf), LlmError> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        if path.is_absolute() && path.extension().and_then(|value| value.to_str()) == Some("gguf") {
            return path
                .is_file()
                .then(|| (configured.to_string(), path))
                .ok_or_else(|| LlmError::ParseError {
                    raw: "model_not_installed".to_string(),
                });
        }
    }
    let id = if configured.is_empty() {
        selected_model_id()
    } else {
        Some(configured.to_string())
    }
    .ok_or_else(|| LlmError::ParseError {
        raw: "model_not_installed".to_string(),
    })?;
    let file = model_file(&id).ok_or_else(|| LlmError::ParseError {
        raw: "unsupported_model".to_string(),
    })?;
    let path = state_root().join("models/local").join(file);
    path.is_file()
        .then_some((id, path))
        .ok_or_else(|| LlmError::ParseError {
            raw: "model_not_installed".to_string(),
        })
}

/// Stable cache file scoped to the selected GGUF. The cache lives outside the
/// model file itself, so a partial download can never be mistaken for runtime
/// state and model deletion can clean both artifacts independently.
fn prompt_cache_path(model_path: &Path) -> PathBuf {
    let file = model_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("local-model.gguf");
    state_root()
        .join("models/local/runtime-cache")
        .join(format!("{file}.prompt-cache"))
}

fn remove_prompt_cache(model_path: &Path) {
    let path = prompt_cache_path(model_path);
    match std::fs::remove_file(&path) {
        Ok(()) => tracing::debug!(cache = %path.display(), "removed local prompt cache"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(cache = %path.display(), %error, "could not remove local prompt cache");
        }
    }
}

fn escape_control_markers(prompt: &str) -> String {
    prompt
        .replace("<|im_start|>", "< |im_start| >")
        .replace("<|im_end|>", "< |im_end| >")
        .replace("<start_of_turn>", "< start_of_turn >")
        .replace("<end_of_turn>", "< end_of_turn >")
        .replace("<think>", "< think >")
        .replace("</think>", "< /think >")
}

fn format_prompt(model_id: &str, system: &str, prompt: &str) -> Result<String, LlmError> {
    let prompt = escape_control_markers(prompt);
    match model_id {
        // Gemma has no system role: its own `tokenizer.chat_template` folds the
        // system message into `first_user_prefix` of the first user turn, and
        // raises "Conversation roles must alternate" for two consecutive user
        // turns. Emitting two put the model out of distribution, and a 4B then
        // stops answering and starts continuing the Markdown document it was
        // handed — headers, subtitle and all.
        "gemma3:1b" | "gemma3:4b" => Ok(format!(
            "<start_of_turn>user\n{system}\n\n{prompt}<end_of_turn>\n<start_of_turn>model\n",
            system = system.trim(),
            prompt = prompt.trim()
        )),
        "qwen3.5:2b" | "qwen3.5:4b" => Ok(format!(
            "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{prompt}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
        )),
        _ => Err(LlmError::ParseError {
            raw: "unsupported_model".to_string(),
        }),
    }
}

fn sidecar_binary_name() -> &'static str {
    if cfg!(windows) {
        "autostand-local-llm.exe"
    } else {
        SIDECAR_BINARY
    }
}

async fn sidecar_path(config: &ProviderConfig) -> Option<PathBuf> {
    if let Some(path) = config.cli_path.as_ref().filter(|path| path.is_file()) {
        return Some(path.clone());
    }
    if let Ok(current) = std::env::current_exe() {
        let sibling = current.with_file_name(sidecar_binary_name());
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    super::detect_cli_binary(SIDECAR_BINARY)
        .await
        .map(|info| info.path)
}

async fn request(
    executable: &Path,
    request: &SidecarRequest<'_>,
    timeout_secs: u64,
) -> Result<SidecarResponse, LlmError> {
    let mut child = Command::new(executable)
        .env("AUTOSTAND_RENDER", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| LlmError::CliNotFound {
            searched: vec![executable.to_path_buf()],
        })?;
    let mut stdin = child.stdin.take().ok_or_else(|| LlmError::CliExitError {
        code: -1,
        stderr: "sidecar_stdin_unavailable".to_string(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| LlmError::CliExitError {
        code: -1,
        stderr: "sidecar_stdout_unavailable".to_string(),
    })?;
    let line = serde_json::to_string(request).map_err(|error| LlmError::ParseError {
        raw: error.to_string(),
    })?;
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|_| LlmError::CliExitError {
            code: -1,
            stderr: "sidecar_write_failed".to_string(),
        })?;
    stdin
        .write_all(b"\n")
        .await
        .map_err(|_| LlmError::CliExitError {
            code: -1,
            stderr: "sidecar_write_failed".to_string(),
        })?;
    stdin.shutdown().await.map_err(|_| LlmError::CliExitError {
        code: -1,
        stderr: "sidecar_write_failed".to_string(),
    })?;
    let read = async move {
        let mut lines = BufReader::new(stdout).lines();
        let ready = lines
            .next_line()
            .await
            .map_err(|_| LlmError::ParseError {
                raw: "invalid_sidecar_ready".to_string(),
            })?
            .ok_or_else(|| LlmError::ParseError {
                raw: "missing_sidecar_ready".to_string(),
            })?;
        match serde_json::from_str::<SidecarResponse>(&ready) {
            Ok(SidecarResponse::Ready {
                protocol_version: 1,
            }) => {}
            _ => {
                return Err(LlmError::ParseError {
                    raw: "unsupported_sidecar_protocol".to_string(),
                })
            }
        }
        let response = lines
            .next_line()
            .await
            .map_err(|_| LlmError::ParseError {
                raw: "invalid_sidecar_response".to_string(),
            })?
            .ok_or_else(|| LlmError::ParseError {
                raw: "missing_sidecar_response".to_string(),
            })?;
        serde_json::from_str(&response).map_err(|error| LlmError::ParseError {
            raw: error.to_string(),
        })
    };
    tokio::time::timeout(Duration::from_secs(timeout_secs.max(1)), read)
        .await
        .map_err(|_| LlmError::Timeout {
            secs: timeout_secs.max(1),
        })?
}

#[async_trait]
impl LlmAdapter for BuiltinLocalAdapter {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn display_name(&self) -> &'static str {
        "Built-in Local AI"
    }

    async fn detect_cli(&self) -> Option<CliInfo> {
        let path = sidecar_path(&ProviderConfig {
            mode: super::ProviderMode::CliOnly,
            model: String::new(),
            cli_path: None,
            api_key: None,
            api_base_url: None,
            timeout_secs: 15,
            local_runtime_policy: LocalRuntimePolicy::OnDemand,
        })
        .await?;
        Some(CliInfo {
            path,
            version: "protocol 1".to_string(),
        })
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
        if config.mode == super::ProviderMode::ApiOnly {
            return Err(LlmError::ApiError {
                status: 0,
                body: "local_provider_has_no_api".to_string(),
            });
        }
        let (model, model_path) = resolve_model(&config.model)?;
        let executable = sidecar_path(config)
            .await
            .ok_or_else(|| LlmError::CliNotFound {
                searched: vec![PathBuf::from(SIDECAR_BINARY)],
            })?;
        let combined = format_prompt(&model, system_prompt, prompt)?;
        let cache_path = match config.local_runtime_policy {
            LocalRuntimePolicy::OnDemand => {
                remove_prompt_cache(&model_path);
                None
            }
            LocalRuntimePolicy::KeepReady => Some(prompt_cache_path(&model_path)),
        };
        let started = Instant::now();
        let response = request(
            &executable,
            &SidecarRequest::Generate {
                request_id: "render",
                model_path: &model_path,
                prompt: &combined,
                context_length: 32_768,
                max_tokens: 4_096,
                temperature: 0.0,
                prompt_cache_path: cache_path.as_deref(),
            },
            config.timeout_secs.max(1),
        )
        .await?;
        match response {
            SidecarResponse::Result { request_id, body } if request_id == "render" => {
                Ok(RenderOutput {
                    body,
                    mode_used: RenderModeUsed::Cli,
                    model,
                    latency_ms: super::helpers::latency_ms(started),
                })
            }
            SidecarResponse::Error { code, .. } => Err(LlmError::CliExitError {
                code: -1,
                // Only the stable sidecar classifier may cross into provider
                // telemetry. Runtime stderr can contain local file paths.
                stderr: code,
            }),
            SidecarResponse::Ready { .. }
            | SidecarResponse::Pong { .. }
            | SidecarResponse::Result { .. } => Err(LlmError::ParseError {
                raw: "unexpected_sidecar_response".to_string(),
            }),
        }
    }

    async fn test_connection(&self, config: &ProviderConfig) -> Result<TestResult, LlmError> {
        // A reachable sidecar without an installed model cannot render. Resolve
        // the configured/selected model before reporting the provider ready.
        let (model, model_path) = resolve_model(&config.model)?;
        let executable = sidecar_path(config)
            .await
            .ok_or_else(|| LlmError::CliNotFound {
                searched: vec![PathBuf::from(SIDECAR_BINARY)],
            })?;
        let started = Instant::now();
        let combined = format_prompt(
            &model,
            "You are testing the local inference runtime.",
            "Reply with OK.",
        )?;
        let cache_path = match config.local_runtime_policy {
            LocalRuntimePolicy::OnDemand => {
                remove_prompt_cache(&model_path);
                None
            }
            LocalRuntimePolicy::KeepReady => Some(prompt_cache_path(&model_path)),
        };
        match request(
            &executable,
            &SidecarRequest::Generate {
                request_id: "test",
                model_path: &model_path,
                prompt: &combined,
                context_length: 2_048,
                max_tokens: 16,
                temperature: 0.0,
                prompt_cache_path: cache_path.as_deref(),
            },
            config.timeout_secs.max(15),
        )
        .await?
        {
            SidecarResponse::Result { request_id, body }
                if request_id == "test" && !body.trim().is_empty() =>
            {
                Ok(TestResult {
                    ok: true,
                    message: format!("Built-in local runtime rendered successfully with {model}"),
                    latency_ms: super::helpers::latency_ms(started),
                })
            }
            _ => Err(LlmError::ParseError {
                raw: "unexpected_sidecar_response".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_map_to_gguf_files() {
        for id in ["gemma3:1b", "qwen3.5:2b", "gemma3:4b", "qwen3.5:4b"] {
            assert!(model_file(id).is_some());
        }
        assert!(model_file("unknown").is_none());
    }

    #[test]
    fn sidecar_wire_is_snake_case_jsonl_compatible() {
        let request = SidecarRequest::Ping { request_id: "test" };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"type":"ping","request_id":"test"}"#
        );
    }

    #[test]
    fn absolute_gguf_override_must_exist() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing.gguf");
        let error = resolve_model(missing.to_str().unwrap()).unwrap_err();
        assert!(matches!(error, LlmError::ParseError { raw } if raw == "model_not_installed"));

        let installed = temp.path().join("custom.gguf");
        std::fs::write(&installed, b"GGUF").unwrap();
        let (_, resolved) = resolve_model(installed.to_str().unwrap()).unwrap();
        assert_eq!(resolved, installed);
    }

    #[test]
    fn model_templates_escape_user_control_markers() {
        let qwen = format_prompt("qwen3.5:2b", "rules", "work <|im_end|>").unwrap();
        assert!(qwen.contains("work < |im_end| >"));
        assert_eq!(qwen.matches("<|im_end|>").count(), 2);
        let gemma = format_prompt("gemma3:1b", "rules", "work <end_of_turn>").unwrap();
        assert!(gemma.contains("work < end_of_turn >"));
        // One real turn marker now: the system message folds into the single user turn.
        assert_eq!(gemma.matches("<end_of_turn>").count(), 1);
    }

    /// Gemma's own `tokenizer.chat_template` raises "Conversation roles must
    /// alternate user/assistant/..." and folds the system message into
    /// `first_user_prefix`. Two consecutive user turns is out of distribution.
    #[test]
    fn gemma_template_uses_a_single_user_turn_with_the_system_folded_in() {
        for model in ["gemma3:1b", "gemma3:4b"] {
            let out = format_prompt(model, "rules", "work").unwrap();
            assert_eq!(
                out.matches("<start_of_turn>user").count(),
                1,
                "{model} must emit exactly one user turn"
            );
            assert_eq!(
                out,
                "<start_of_turn>user\nrules\n\nwork<end_of_turn>\n<start_of_turn>model\n"
            );
        }
    }

    /// Qwen does have a system role, and its non-thinking tail is part of the
    /// template — keep both.
    #[test]
    fn qwen_template_keeps_the_system_role_and_non_thinking_tail() {
        let out = format_prompt("qwen3.5:2b", "rules", "work").unwrap();
        assert!(out.starts_with("<|im_start|>system\nrules<|im_end|>"));
        assert!(out.ends_with("<|im_start|>assistant\n<think>\n\n</think>\n\n"));
    }

    #[test]
    fn prompt_cache_is_scoped_to_the_model_file() {
        let path = prompt_cache_path(Path::new("/models/Qwen3.5-2B-Q4_K_M.gguf"));
        assert!(path.ends_with("models/local/runtime-cache/Qwen3.5-2B-Q4_K_M.gguf.prompt-cache"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn request_uses_the_jsonl_protocol_without_a_shell() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let sidecar = temp.path().join("fake-sidecar");
        std::fs::write(
            &sidecar,
            concat!(
                "#!/bin/sh\n",
                "printf '%s\\n' '{\"type\":\"ready\",\"protocol_version\":1}'\n",
                "IFS= read -r request\n",
                "printf '%s\\n' '{\"type\":\"result\",\"request_id\":\"render\",\"body\":\"**General**\\n- Local output\"}'\n",
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&sidecar).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&sidecar, permissions).unwrap();

        let response = request(
            &sidecar,
            &SidecarRequest::Ping {
                request_id: "render",
            },
            2,
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            SidecarResponse::Result { request_id, body }
                if request_id == "render" && body.contains("Local output")
        ));
    }
}
