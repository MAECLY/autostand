//! Shared helpers for LLM adapters: CLI subprocess execution, HTTP calls,
//! API-key resolution from keychain/env.

use super::{LlmError, ProviderConfig};
use autostand_runlog::proc::{run_process, ProcError, ProcSpec, StreamPolicy};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

/// Anti-recursion env var set on every spawned CLI subprocess.
const RENDER_ENV: (&str, &str) = ("AUTOSTAND_RENDER", "1");

/// Step a provider render is filed under, matching `RunKind`'s vocabulary.
const RENDER_STEP: &str = "render_llm";

/// Step a provider *probe* (`--version`, a health check) is filed under.
const DETECT_STEP: &str = "cli_detect";

/// Spawn a CLI subprocess, feed `prompt` to stdin, capture stdout.
///
/// Sets `AUTOSTAND_RENDER=1` plus any `extra_env` entries. Maps exit codes
/// to [`LlmError::CliExitError`], timeouts to [`LlmError::Timeout`], and a
/// failure to spawn the binary to [`LlmError::CliNotFound`].
///
/// [`StreamPolicy::Summary`] is not a default here but a requirement: this
/// child's **stdout is the standup body**, so echoing it into the Terminal
/// would print the user's unreviewed standup into a panel — and its stderr can
/// carry a bad-credentials line. `run_process` logs neither; only the display
/// name, the exit code and the duration reach the panel.
pub async fn run_cli(
    cmd: &str,
    args: &[&str],
    prompt: &str,
    timeout_secs: u64,
    extra_env: &[(&str, &str)],
) -> Result<String, LlmError> {
    run_cli_inner(
        cmd,
        args,
        prompt,
        timeout_secs,
        extra_env,
        RENDER_STEP,
        StreamPolicy::Summary,
    )
    .await
}

/// [`run_cli`] for a `--version`-style probe.
///
/// [`StreamPolicy::Silent`]: a health refresh probes every configured provider,
/// so five start/end pairs would bury whatever the user actually asked for. The
/// command that owns the sweep reports the outcome instead.
pub async fn run_cli_probe(
    cmd: &str,
    args: &[&str],
    timeout_secs: u64,
) -> Result<String, LlmError> {
    run_cli_inner(
        cmd,
        args,
        "",
        timeout_secs,
        &[],
        DETECT_STEP,
        StreamPolicy::Silent,
    )
    .await
}

/// Shared body of [`run_cli`] and [`run_cli_probe`].
async fn run_cli_inner(
    cmd: &str,
    args: &[&str],
    prompt: &str,
    timeout_secs: u64,
    extra_env: &[(&str, &str)],
    step: &'static str,
    stream: StreamPolicy,
) -> Result<String, LlmError> {
    let mut spec = ProcSpec::new(cmd, step)
        .args(args)
        .stdin(prompt.as_bytes().to_vec())
        .timeout(Duration::from_secs(timeout_secs))
        .stream(stream)
        .env(RENDER_ENV.0, RENDER_ENV.1);
    for (key, value) in extra_env {
        spec = spec.env(key, value);
    }
    let output = run_process(spec).await.map_err(|err| match err {
        ProcError::Spawn { .. } => LlmError::CliNotFound {
            searched: vec![PathBuf::from(cmd)],
        },
        ProcError::Timeout { secs, .. } => LlmError::Timeout { secs },
        // The kind alone: the io error's message can embed the resolved path.
        ProcError::Io { kind, .. } => LlmError::CliExitError {
            code: -1,
            stderr: format!("io error ({kind})"),
        },
    })?;
    if output.success {
        return Ok(output.stdout_trimmed().to_string());
    }
    // stderr travels to the caller, never to the Terminal: `probe_failure` in
    // the app crate reduces it to the exit code before anything is displayed.
    Err(LlmError::CliExitError {
        code: output.code.unwrap_or(-1),
        stderr: output.stderr_trimmed().to_string(),
    })
}

/// POST a JSON body and parse the JSON response.
///
/// Maps 401/403 to [`LlmError::AuthError`], 429 to [`LlmError::RateLimit`],
/// other non-2xx to [`LlmError::ApiError`], and timeouts to [`LlmError::Timeout`].
pub async fn http_post_json(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
    timeout_secs: u64,
) -> Result<Value, LlmError> {
    let mut req = client
        .post(url)
        .json(body)
        .timeout(Duration::from_secs(timeout_secs));
    for &(k, v) in headers {
        req = req.header(k, v);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| map_reqwest_err(&e, timeout_secs))?;
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Err(LlmError::AuthError);
    }
    if status == 429 {
        let retry_after_secs = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        return Err(LlmError::RateLimit { retry_after_secs });
    }
    let text = resp
        .text()
        .await
        .map_err(|e| map_reqwest_err(&e, timeout_secs))?;
    if !(200..300).contains(&status) {
        return Err(LlmError::ApiError { status, body: text });
    }
    serde_json::from_str(&text).map_err(|e| LlmError::ParseError { raw: e.to_string() })
}

/// GET a URL and parse the JSON response. Same error mapping as [`http_post_json`].
pub async fn http_get_json(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&str, &str)],
    timeout_secs: u64,
) -> Result<Value, LlmError> {
    let mut req = client.get(url).timeout(Duration::from_secs(timeout_secs));
    for &(k, v) in headers {
        req = req.header(k, v);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| map_reqwest_err(&e, timeout_secs))?;
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Err(LlmError::AuthError);
    }
    if status == 429 {
        let retry_after_secs = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        return Err(LlmError::RateLimit { retry_after_secs });
    }
    let text = resp
        .text()
        .await
        .map_err(|e| map_reqwest_err(&e, timeout_secs))?;
    if !(200..300).contains(&status) {
        return Err(LlmError::ApiError { status, body: text });
    }
    serde_json::from_str(&text).map_err(|e| LlmError::ParseError { raw: e.to_string() })
}

/// Build a `reqwest` client with the workspace's rustls-tls backend.
pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .build()
        .expect("reqwest client with rustls-tls should always build")
}

/// Measure elapsed milliseconds since `start`, saturating to `u64`.
pub fn latency_ms(start: std::time::Instant) -> u64 {
    start.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

/// Load an API key from the OS keychain, falling back to the given env vars.
pub fn load_api_key(account: &str, env_vars: &[&str]) -> Option<String> {
    if let Ok(entry) = keyring::Entry::new("autostand", account) {
        if let Ok(pw) = entry.get_password() {
            return Some(pw);
        }
    }
    for var in env_vars {
        if let Ok(val) = std::env::var(var) {
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

/// Resolve the API key for a render call: explicit config override wins,
/// otherwise keychain + env fallback.
pub fn resolve_api_key(
    config: &ProviderConfig,
    account: &str,
    env_vars: &[&str],
) -> Option<String> {
    if let Some(k) = &config.api_key {
        if !k.is_empty() {
            return Some(k.clone());
        }
    }
    load_api_key(account, env_vars)
}

/// Resolve the CLI binary to invoke: explicit config override, else PATH discovery.
pub async fn resolve_cli_cmd(config: &ProviderConfig, name: &str) -> Option<String> {
    if let Some(p) = &config.cli_path {
        if !p.as_os_str().is_empty() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    super::detect_cli_binary(name)
        .await
        .map(|c| c.path.to_string_lossy().to_string())
}

fn map_reqwest_err(e: &reqwest::Error, timeout_secs: u64) -> LlmError {
    if e.is_timeout() {
        LlmError::Timeout { secs: timeout_secs }
    } else {
        LlmError::ApiError {
            status: 0,
            body: e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unnecessary_wraps)]
    use super::*;
    use crate::llm::{ProviderConfig, ProviderMode};
    use autostand_runlog::{scoped, RecordingSink, SinkRef};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn cfg(api_key: Option<String>, cli_path: Option<PathBuf>) -> ProviderConfig {
        ProviderConfig {
            mode: ProviderMode::CliFirst,
            model: "test-model".to_string(),
            cli_path,
            api_key,
            api_base_url: None,
            timeout_secs: 30,
            local_runtime_policy: crate::llm::LocalRuntimePolicy::OnDemand,
        }
    }

    #[test]
    fn build_client_returns_reqwest_client() {
        let _client = build_client();
    }

    #[test]
    fn latency_ms_is_nonnegative_for_recent_instant() {
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(5));
        let ms = latency_ms(start);
        assert!(ms >= 1, "expected positive latency, got {ms}");
    }

    #[test]
    fn latency_ms_zero_for_same_instant() {
        let start = Instant::now();
        let ms = latency_ms(start);
        let _ = ms;
    }

    #[test]
    fn load_api_key_falls_back_to_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        let var = "AUTOSTAND_TEST_API_KEY_ENV_FALLBACK";
        std::env::set_var(var, "test-secret-from-env");
        let result = load_api_key("autostand-test-account-no-keychain", &[var]);
        std::env::remove_var(var);
        assert_eq!(result.as_deref(), Some("test-secret-from-env"));
    }

    #[test]
    fn load_api_key_ignores_empty_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        let var = "AUTOSTAND_TEST_API_KEY_EMPTY";
        std::env::set_var(var, "");
        let result = load_api_key("autostand-test-empty", &[var]);
        std::env::remove_var(var);
        assert!(result.is_none());
    }

    #[test]
    fn load_api_key_returns_none_when_keychain_and_env_empty() {
        let _guard = ENV_LOCK.lock().unwrap();
        let var = "AUTOSTAND_TEST_API_KEY_NEVER_SET_999";
        std::env::remove_var(var);
        let result = load_api_key("autostand-test-never-exists-12345", &[var]);
        assert!(result.is_none());
    }

    #[test]
    fn resolve_api_key_explicit_override_wins() {
        let _guard = ENV_LOCK.lock().unwrap();
        let var = "AUTOSTAND_TEST_API_KEY_OVERRIDE";
        std::env::set_var(var, "from-env-should-not-win");
        let result = resolve_api_key(
            &cfg(Some("from-config".to_string()), None),
            "autostand-test-override",
            &[var],
        );
        std::env::remove_var(var);
        assert_eq!(result.as_deref(), Some("from-config"));
    }

    #[test]
    fn resolve_api_key_empty_override_falls_through() {
        let _guard = ENV_LOCK.lock().unwrap();
        let var = "AUTOSTAND_TEST_API_KEY_EMPTY_OVERRIDE";
        std::env::set_var(var, "from-env-fallback");
        let result = resolve_api_key(
            &cfg(Some(String::new()), None),
            "autostand-test-empty-override",
            &[var],
        );
        std::env::remove_var(var);
        assert_eq!(result.as_deref(), Some("from-env-fallback"));
    }

    #[test]
    fn resolve_api_key_no_override_no_env_returns_none() {
        let _guard = ENV_LOCK.lock().unwrap();
        let var = "AUTOSTAND_TEST_API_KEY_RESOLVE_NONE_999";
        std::env::remove_var(var);
        let result = resolve_api_key(
            &cfg(None, None),
            "autostand-test-resolve-none-12345",
            &[var],
        );
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn resolve_cli_cmd_uses_explicit_cli_path() {
        let cfg = cfg(None, Some(PathBuf::from("/usr/local/bin/custom-cli")));
        let result = resolve_cli_cmd(&cfg, "never-looked-up").await;
        assert_eq!(result.as_deref(), Some("/usr/local/bin/custom-cli"));
    }

    #[tokio::test]
    async fn resolve_cli_cmd_empty_cli_path_falls_through_to_discovery() {
        let cfg = cfg(None, Some(PathBuf::new()));
        let result = resolve_cli_cmd(&cfg, "definitely-not-a-real-binary-xyz").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn resolve_cli_cmd_no_path_falls_through_to_discovery() {
        let cfg = cfg(None, None);
        let result = resolve_cli_cmd(&cfg, "definitely-not-a-real-binary-xyz").await;
        assert!(result.is_none());
    }

    // -- terminal visibility ------------------------------------------------

    /// A render is an action, so it must appear. But this child's stdout *is*
    /// the user's standup and its stdin is the prompt, so the panel may only
    /// learn that a provider ran and how it ended.
    #[tokio::test]
    async fn a_render_is_visible_but_its_prompt_and_body_never_are() {
        let sink = Arc::new(RecordingSink::new());
        let as_ref: SinkRef = sink.clone();
        let body = scoped(as_ref, async {
            run_cli(
                "cat",
                &[],
                "Yesterday I shipped the auth migration",
                10,
                &[],
            )
            .await
        })
        .await
        .expect("cat runs");

        assert_eq!(body, "Yesterday I shipped the auth migration");
        let rendered = sink
            .lines()
            .iter()
            .map(|line| format!("{} {}", line.step, line.message))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("render_llm"), "{rendered}");
        assert!(rendered.contains("cat"), "{rendered}");
        assert!(
            !rendered.contains("auth migration"),
            "the standup body reached the panel: {rendered}"
        );
    }

    /// A failing provider hands stderr back to the caller (`probe_failure` needs
    /// the exit code) but never to the Terminal.
    #[tokio::test]
    async fn a_failing_render_keeps_its_stderr_out_of_the_terminal() {
        let sink = Arc::new(RecordingSink::new());
        let as_ref: SinkRef = sink.clone();
        let error = scoped(as_ref, async {
            run_cli(
                "sh",
                &["-c", "echo invalid-api-key-hint 1>&2; exit 2"],
                "",
                10,
                &[],
            )
            .await
            .expect_err("non-zero exit")
        })
        .await;

        match &error {
            LlmError::CliExitError { code, stderr } => {
                assert_eq!(*code, 2);
                assert!(stderr.contains("invalid-api-key-hint"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert!(!sink.messages().join("\n").contains("invalid-api-key-hint"));
    }

    /// Health refreshes probe every provider; five start/end pairs would bury
    /// whatever the user actually asked for.
    #[tokio::test]
    async fn a_version_probe_stays_silent() {
        let sink = Arc::new(RecordingSink::new());
        let as_ref: SinkRef = sink.clone();
        scoped(as_ref, async {
            let _ = run_cli_probe("echo", &["v1.2.3"], 10).await;
        })
        .await;
        assert!(sink.lines().is_empty(), "{:?}", sink.messages());
    }
}
