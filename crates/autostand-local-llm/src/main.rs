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
        /// Optional llama.cpp prompt/KV cache. Absence is true one-shot mode.
        #[serde(default)]
        prompt_cache_path: Option<PathBuf>,
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
                runtime_binary_names()
                    .into_iter()
                    .map(|name| executable.with_file_name(name))
                    .find(|candidate| candidate.is_file())
            })
        })
        // Source/development builds intentionally do not copy Homebrew or a
        // distro-provided llama-cli into target/debug. Resolve it from PATH
        // without invoking a shell; production bundles still win via sibling.
        .or_else(|| {
            std::env::var_os("PATH").and_then(|path| {
                std::env::split_paths(&path)
                    .flat_map(|directory| {
                        runtime_binary_names()
                            .into_iter()
                            .map(move |name| directory.join(name))
                    })
                    .find(|candidate| candidate.is_file())
            })
        })
}

/// New llama.cpp releases split the one-shot completion program out of
/// `llama-cli`; older/pinned releases expose the same flags on `llama-cli`.
/// Prefer the dedicated binary when present while retaining release-bundle
/// compatibility.
const fn runtime_binary_names() -> [&'static str; 2] {
    if cfg!(windows) {
        ["llama-completion.exe", "llama-cli.exe"]
    } else {
        ["llama-completion", "llama-cli"]
    }
}

fn generate(
    model_path: &Path,
    prompt: &str,
    context_length: u32,
    max_tokens: u32,
    temperature: f32,
    prompt_cache_path: Option<&Path>,
) -> Result<String, RuntimeError> {
    if !model_path.is_file()
        || model_path.extension().and_then(|value| value.to_str()) != Some("gguf")
    {
        return Err(RuntimeError::InvalidModel);
    }
    let runtime = runtime_path().ok_or(RuntimeError::RuntimeMissing)?;
    let dedicated_completion = runtime
        .file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "llama-completion");
    if let Some(cache_path) = prompt_cache_path {
        prepare_cache_parent(cache_path).map_err(RuntimeError::Spawn)?;
    }
    let mut command = Command::new(runtime);
    command
        .arg("--model")
        .arg(model_path)
        .arg("--prompt")
        .arg(prompt)
        .arg("--ctx-size")
        .arg(context_length.clamp(2_048, 32_768).to_string())
        .arg("--n-predict")
        .arg(max_tokens.clamp(1, 4_096).to_string())
        .arg("--temp")
        .arg(temperature.clamp(0.0, 2.0).to_string())
        .arg("--no-display-prompt")
        // Recent llama.cpp releases infer conversation mode from a model's
        // chat template. Explicitly disable it so n-predict ends the process;
        // --single-turn would enable the chat UI and echo banners/prompts.
        .arg("--no-conversation")
        .arg("--simple-io")
        // `--escape` defaults to true, so llama.cpp reinterprets `\n`, `\"` and
        // `\\` inside the prompt. Gathered notes and code snippets contain those
        // sequences literally; the prompt must reach the model byte-for-byte.
        .arg("--no-escape")
        // Greedy decoding (--temp 0) with penalties disabled makes small models
        // loop a section verbatim — the observed `**Blockers**\n- None` twice.
        // DRY breaks long repeats; `--dry-allowed-length 6` leaves short legitimate
        // ones (a second `- None` under a different header) untouched. The
        // deterministic dedupe in `render::sanitize_body` remains the real
        // guarantee; this only makes it rarer for the sampler to get there.
        .arg("--dry-multiplier")
        .arg("0.8")
        .arg("--dry-base")
        .arg("1.75")
        .arg("--dry-allowed-length")
        .arg("6")
        .arg("--dry-penalty-last-n")
        .arg("512");
    if !dedicated_completion {
        // The legacy unified CLI writes its banner through the logger. The
        // dedicated completion program also writes generated tokens through
        // that channel, so disabling it there would produce an empty body.
        command.arg("--log-disable");
    }
    if let Some(cache_path) = prompt_cache_path {
        // Do not use --prompt-cache-all: generated standup text should not be
        // retained, and the stable system/prompt prefix is the useful part.
        command.arg("--prompt-cache").arg(cache_path);
    }
    let output = command
        .env("AUTOSTAND_RENDER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(RuntimeError::Spawn)?;
    if let Some(cache_path) = prompt_cache_path {
        secure_cache_permissions(cache_path);
    }
    if output.status.success() {
        Ok(strip_runtime_markers(&String::from_utf8_lossy(
            &output.stdout,
        )))
    } else {
        Err(RuntimeError::Exit {
            code: output.status.code().unwrap_or(-1),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

/// Control tokens a chat-tuned GGUF can emit before the runtime stops it.
const CONTROL_TOKENS: &[&str] = &[
    "<end_of_turn>",
    "<|im_end|>",
    "<|endoftext|>",
    "<|eot_id|>",
    "</s>",
    "<eos>",
];

/// Strip llama.cpp's own stdout decorations from a completion.
///
/// `llama-completion` prints a literal ` [end of text]` when generation stops at
/// an end-of-generation token, and it does so on stdout — the same stream the
/// tokens go to, so `--log-disable` cannot be used to suppress it without also
/// silencing the completion itself. Trimming alone leaves the marker inside the
/// body, which is how it reached committed standup files.
fn strip_runtime_markers(stdout: &str) -> String {
    let mut out = stdout.replace("[end of text]", "");
    for token in CONTROL_TOKENS {
        out = out.replace(token, "");
    }
    // The legacy unified `llama-cli` can still emit a boxed ASCII banner ahead of
    // the completion. Its lines are pure box-drawing, so they are safe to drop.
    let cleaned: Vec<&str> = out
        .lines()
        .filter(|line| !is_banner_line(line))
        .map(str::trim_end)
        .collect();
    cleaned.join("\n").trim().to_string()
}

/// True for a line made only of box-drawing characters and whitespace.
///
/// Deliberately limited to box drawing: `---` and `===` are legitimate Markdown
/// and must survive.
fn is_banner_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| matches!(c, '─' | '│' | '┌' | '┐' | '└' | '┘' | '├' | '┤' | '━' | ' '))
}

fn prepare_cache_parent(path: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

fn secure_cache_permissions(path: &Path) {
    #[cfg(unix)]
    if path.is_file() {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            eprintln!("could not restrict prompt cache permissions: {error}");
        }
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
                    prompt_cache_path,
                }) => match generate(
                    &model_path,
                    &prompt,
                    context_length,
                    max_tokens,
                    temperature,
                    prompt_cache_path.as_deref(),
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

    #[cfg(unix)]
    use std::sync::Mutex;

    #[cfg(unix)]
    static ENV_LOCK: Mutex<()> = Mutex::new(());

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
        let error =
            generate(Path::new("missing.gguf"), "prompt", 4_096, 16, 0.0, None).unwrap_err();
        assert!(matches!(error, RuntimeError::InvalidModel));
    }

    #[cfg(unix)]
    #[test]
    fn prompt_cache_is_forwarded_only_when_requested() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = ENV_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("llama-cli");
        let args = temp.path().join("args.txt");
        let model = temp.path().join("model.gguf");
        let cache = temp.path().join("cache/model.prompt-cache");
        std::fs::write(
            &runtime,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$AUTOSTAND_TEST_LLAMA_ARGS\"\nprintf 'OK\\n'\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&runtime).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&runtime, permissions).unwrap();
        std::fs::write(&model, b"GGUF").unwrap();
        std::env::set_var("AUTOSTAND_LLAMA_CLI", &runtime);
        std::env::set_var("AUTOSTAND_TEST_LLAMA_ARGS", &args);

        let body = generate(&model, "hello", 2_048, 1, 0.0, Some(&cache)).unwrap();
        assert_eq!(body, "OK");
        let forwarded = std::fs::read_to_string(&args).unwrap();
        assert!(forwarded.contains("--prompt-cache\n"));
        assert!(forwarded.contains(cache.to_str().unwrap()));
        assert!(cache.parent().unwrap().is_dir());

        let _ = std::fs::remove_file(&args);
        let _ = generate(&model, "hello", 2_048, 1, 0.0, None).unwrap();
        let forwarded = std::fs::read_to_string(&args).unwrap();
        assert!(!forwarded.contains("--prompt-cache\n"));

        std::env::remove_var("AUTOSTAND_LLAMA_CLI");
        std::env::remove_var("AUTOSTAND_TEST_LLAMA_ARGS");
    }

    /// `llama-completion` prints ` [end of text]` on stdout, the same stream the
    /// tokens go to. Trimming alone left it inside the body and it reached
    /// committed standup files.
    #[test]
    fn strip_runtime_markers_removes_the_end_of_text_marker() {
        assert_eq!(strip_runtime_markers("OK\n [end of text]\n\n\n"), "OK");
        assert_eq!(
            strip_runtime_markers("**Blockers**\n- None\n [end of text]\n"),
            "**Blockers**\n- None"
        );
    }

    #[test]
    fn strip_runtime_markers_removes_leaked_control_tokens() {
        assert_eq!(strip_runtime_markers("body<end_of_turn>"), "body");
        assert_eq!(strip_runtime_markers("body<|im_end|>\n"), "body");
        assert_eq!(strip_runtime_markers("body</s>"), "body");
    }

    #[test]
    fn strip_runtime_markers_drops_a_box_drawing_banner_but_keeps_markdown_rules() {
        assert_eq!(
            strip_runtime_markers("┌────────┐\n│        │\nreal body\n"),
            "real body"
        );
        assert_eq!(
            strip_runtime_markers("above\n---\nbelow"),
            "above\n---\nbelow"
        );
        assert_eq!(
            strip_runtime_markers("above\n===\nbelow"),
            "above\n===\nbelow"
        );
    }

    #[test]
    fn strip_runtime_markers_is_idempotent() {
        let once = strip_runtime_markers("OK\n [end of text]\n");
        assert_eq!(strip_runtime_markers(&once), once);
    }

    #[cfg(unix)]
    #[test]
    fn runtime_args_disable_escapes_and_enable_anti_repetition() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = ENV_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("llama-cli");
        let args = temp.path().join("args.txt");
        let model = temp.path().join("model.gguf");
        std::fs::write(
            &runtime,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$AUTOSTAND_TEST_LLAMA_ARGS\"\nprintf 'OK\\n'\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&runtime).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&runtime, permissions).unwrap();
        std::fs::write(&model, b"GGUF").unwrap();
        std::env::set_var("AUTOSTAND_LLAMA_CLI", &runtime);
        std::env::set_var("AUTOSTAND_TEST_LLAMA_ARGS", &args);

        let _ = generate(&model, "hello", 2_048, 1, 0.0, None).unwrap();
        let forwarded = std::fs::read_to_string(&args).unwrap();
        // `--escape` defaults to true and would reinterpret `\n` inside notes.
        assert!(forwarded.contains("--no-escape\n"));
        assert!(forwarded.contains("--dry-multiplier\n"));
        assert!(forwarded.contains("--dry-allowed-length\n"));

        std::env::remove_var("AUTOSTAND_LLAMA_CLI");
        std::env::remove_var("AUTOSTAND_TEST_LLAMA_ARGS");
    }
}
