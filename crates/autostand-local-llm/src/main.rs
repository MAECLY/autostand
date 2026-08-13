//! Process-isolated local inference sidecar.
//!
//! The initial runtime delegates to a bundled `llama-cli` binary. Keeping the
//! JSONL protocol independent from llama.cpp bindings avoids exposing FFI to the
//! Tauri process and lets release builds select a platform-specific runtime.

#![forbid(unsafe_code)]

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    Ping {
        request_id: String,
    },
    Generate {
        request_id: String,
        model_path: PathBuf,
        prompt: String,
        #[serde(default = "default_context")]
        context_length: u32,
        #[serde(default = "default_max_tokens")]
        max_tokens: u32,
        #[serde(default)]
        temperature: f32,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Response {
    Ready {
        protocol_version: u32,
    },
    Pong {
        request_id: String,
    },
    Result {
        request_id: String,
        body: String,
    },
    Error {
        request_id: String,
        code: &'static str,
        message: String,
    },
}

#[derive(Debug, thiserror::Error)]
enum RuntimeError {
    #[error("model path is not an installed GGUF file")]
    InvalidModel,
    #[error("llama.cpp runtime was not found")]
    RuntimeMissing,
    #[error("failed to start llama.cpp runtime: {0}")]
    Spawn(std::io::Error),
    #[error("llama.cpp exited with code {code}: {message}")]
    Exit { code: i32, message: String },
}

const fn default_context() -> u32 {
    32_768
}

const fn default_max_tokens() -> u32 {
    4_096
}

fn runtime_path() -> Option<PathBuf> {
    std::env::var_os("AUTOSTAND_LLAMA_CLI")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            std::env::current_exe().ok().and_then(|executable| {
                let sibling = executable.with_file_name(if cfg!(windows) {
                    "llama-cli.exe"
                } else {
                    "llama-cli"
                });
                sibling.is_file().then_some(sibling)
            })
        })
}

fn generate(
    model_path: &Path,
    prompt: &str,
    context_length: u32,
    max_tokens: u32,
    temperature: f32,
) -> Result<String, RuntimeError> {
    if !model_path.is_file()
        || model_path.extension().and_then(|value| value.to_str()) != Some("gguf")
    {
        return Err(RuntimeError::InvalidModel);
    }
    let runtime = runtime_path().ok_or(RuntimeError::RuntimeMissing)?;
    let output = Command::new(runtime)
        .args([
            "--model",
            &model_path.to_string_lossy(),
            "--prompt",
            prompt,
            "--ctx-size",
            &context_length.clamp(2_048, 32_768).to_string(),
            "--n-predict",
            &max_tokens.clamp(1, 4_096).to_string(),
            "--temp",
            &temperature.clamp(0.0, 2.0).to_string(),
            "--no-display-prompt",
            "--simple-io",
        ])
        .env("AUTOSTAND_RENDER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(RuntimeError::Spawn)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(RuntimeError::Exit {
            code: output.status.code().unwrap_or(-1),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

fn respond(response: &Response) -> Result<(), std::io::Error> {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, response)?;
    output.write_all(b"\n")?;
    output.flush()
}

fn main() {
    if respond(&Response::Ready {
        protocol_version: 1,
    })
    .is_err()
    {
        return;
    }
    for line in std::io::stdin().lock().lines() {
        let response = match line {
            Ok(line) => match serde_json::from_str::<Request>(&line) {
                Ok(Request::Ping { request_id }) => Response::Pong { request_id },
                Ok(Request::Generate {
                    request_id,
                    model_path,
                    prompt,
                    context_length,
                    max_tokens,
                    temperature,
                }) => match generate(
                    &model_path,
                    &prompt,
                    context_length,
                    max_tokens,
                    temperature,
                ) {
                    Ok(body) => Response::Result { request_id, body },
                    Err(error) => Response::Error {
                        request_id,
                        code: match error {
                            RuntimeError::InvalidModel => "invalid_model",
                            RuntimeError::RuntimeMissing => "runtime_missing",
                            RuntimeError::Spawn(_) => "runtime_spawn_failed",
                            RuntimeError::Exit { .. } => "runtime_failed",
                        },
                        message: error.to_string(),
                    },
                },
                Err(error) => Response::Error {
                    request_id: String::new(),
                    code: "invalid_request",
                    message: error.to_string(),
                },
            },
            Err(error) => Response::Error {
                request_id: String::new(),
                code: "input_error",
                message: error.to_string(),
            },
        };
        if respond(&response).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_defaults_are_bounded_for_standup_rendering() {
        let request: Request = serde_json::from_str(
            r#"{"type":"generate","request_id":"1","model_path":"model.gguf","prompt":"hello"}"#,
        )
        .unwrap();
        match request {
            Request::Generate {
                context_length,
                max_tokens,
                temperature,
                ..
            } => {
                assert_eq!(context_length, 32_768);
                assert_eq!(max_tokens, 4_096);
                assert!(temperature.abs() < f32::EPSILON);
            }
            Request::Ping { .. } => panic!("wrong request variant"),
        }
    }

    #[test]
    fn refuses_missing_models_before_starting_runtime() {
        let error = generate(Path::new("missing.gguf"), "prompt", 4_096, 16, 0.0).unwrap_err();
        assert!(matches!(error, RuntimeError::InvalidModel));
    }
}
