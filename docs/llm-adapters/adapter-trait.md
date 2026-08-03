# The `LlmAdapter` trait

This is the canonical Rust trait every provider implements. It lives in `crates/autostand-adapters/src/llm/mod.rs`.

## Full definition

```rust
use async_trait::async_trait;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};

#[async_trait]
pub trait LlmAdapter: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    async fn detect_cli(&self) -> Option<CliInfo>;
    async fn has_api_key(&self) -> bool;
    async fn render(&self, prompt: &str, system_prompt: &str, config: &ProviderConfig) -> Result<RenderOutput, LlmError>;
    async fn test_connection(&self, config: &ProviderConfig) -> Result<TestResult, LlmError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliInfo {
    pub path: PathBuf,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderOutput {
    pub body: String,
    pub mode_used: RenderModeUsed,
    pub model: String,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderModeUsed { Cli, Api }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmError {
    Timeout { secs: u64 },
    CliNotFound { searched: Vec<PathBuf> },
    CliExitError { code: i32, stderr: String },
    ApiError { status: u16, body: String },
    AuthError,
    ParseError { raw: String },
    RateLimit { retry_after_secs: Option<u64> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
    pub latency_ms: u64,
}

pub enum ProviderMode { CliFirst, CliOnly, ApiOnly, ApiFallback }
```

Notes on the expanded form vs. the overview:

- `render()` takes `system_prompt` explicitly — providers that have no system-prompt concept (Ollama native) fold it into the user message; providers with one (Claude, OpenAI, Gemini) put it in the right field.
- `LlmError` is structured: every variant carries the data needed to render a useful audit-sidecar entry and a user-facing error in the Settings UI. `CliNotFound.searched` lists every path the discovery algorithm checked so the user can see why detection failed.
- `ProviderMode` is a plain enum (no `Default`) — `ProviderConfig` carries the chosen mode and each provider resolves its own default at construction time.

## CLI subprocess execution

All CLI renders go through one helper:

```rust
async fn run_cli(
    cli_path: &Path,
    args: &[&str],
    prompt: &str,
    timeout_secs: u64,
    extra_env: &[(String, String)],
) -> Result<CliRunResult, LlmError>
```

Implementation sketch:

```rust
let mut cmd = tokio::process::Command::new(cli_path);
cmd.args(args)
    .env("AUTOSTAND_RENDER", "1")            // anti-recursion guard
    .envs(extra_env.iter().map(|(k, v)| (k.clone(), v.clone())))
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

let child = cmd.spawn().map_err(|e| LlmError::CliNotFound { searched: vec![cli_path.to_owned()] })?;
let (mut stdin, stdout, stderr) = child.stdin.take().unwrap(); // simplified

// Write the prompt to stdin
let write_task = tokio::spawn(async move {
    let _ = stdin.write_all(prompt.as_bytes()).await;
    let _ = stdin.shutdown().await;
});

// Wait for the process with timeout
let wait = tokio::time::timeout(
    Duration::from_secs(timeout_secs),
    child.wait_with_output(),
).await;

match wait {
    Ok(Ok(out)) if out.status.success() => Ok(CliRunResult { stdout: String::from_utf8_lossy(&out.stdout).into_owned(), stderr: String::from_utf8_lossy(&out.stderr).into_owned() }),
    Ok(Ok(out)) => Err(LlmError::CliExitError { code: out.status.code().unwrap_or(-1), stderr: String::from_utf8_lossy(&out.stderr).into_owned() }),
    Ok(Err(_)) => Err(LlmError::CliNotFound { searched: vec![cli_path.to_owned()] }),
    Err(_) => Err(LlmError::Timeout { secs: timeout_secs }),
}
```

`AUTOSTAND_RENDER=1` is always set on the child env; it is scoped to the `Command` and never leaks into the parent process's environment. Provider-specific extra env (e.g. `CLAUDE_STANDUP_RENDER=1`) is passed via `extra_env`.

## API HTTP execution

All API renders go through a thin helper:

```rust
async fn call_api(
    client: &reqwest::Client,
    url: &str,
    headers: Vec<(&str, String)>,
    body: serde_json::Value,
    timeout_secs: u64,
) -> Result<reqwest::Response, LlmError>
```

The `reqwest::Client` is built once per adapter with `ClientBuilder::timeout(Duration::from_secs(timeout_secs))` and is reused across renders. Headers always include `content-type: application/json` plus the provider's auth header. Response handling maps HTTP status to `LlmError`:

- `200` → parse body, return.
- `401 / 403` → `AuthError`.
- `429` → `RateLimit { retry_after_secs }` (parsed from `Retry-After` header if present).
- other non-2xx → `ApiError { status, body }`.

## CLI discovery algorithm

`detect_cli()` runs the same algorithm for every provider:

1. **Override** — if `ProviderConfig.cli_path` is `Some(p)` and `p` exists and is executable, return `CliInfo { path: p, version: <p --version> }`.
2. **PATH** — `which <binary>` (via the `which` crate). If found, run `<path> --version` and return.
3. **Platform defaults** — check the provider's list of well-known install paths (homebrew, npm global, vendor-specific `~/.<vendor>/bin/`). First match wins.
4. **None** — return `None`. The caller (in `CliFirst`) falls through to API; in `CliOnly` it returns `LlmError::CliNotFound { searched: <all paths checked> }`.

`--version` invocation is itself wrapped in a short timeout (10s) so a hung binary doesn't stall discovery.

## Provider registry

```rust
pub fn registry() -> Vec<Box<dyn LlmAdapter>> {
    vec![
        Box::new(ClaudeAdapter::new()),
        Box::new(OllamaAdapter::new()),
        Box::new(OpenAiAdapter::new()),
        Box::new(GeminiAdapter::new()),
        Box::new(GrokAdapter::new()),
    ]
}
```

`autostand-adapters::llm::registry()` returns one instance per provider. The render pipeline:

1. Loads `config.llm.preferred_provider` (e.g. `"claude"`).
2. Looks up the adapter in the registry by `id()`.
3. Calls `adapter.render(prompt, system_prompt, &config.llm.provider_config)`.
4. Runs `validate_render(body, facts)` (see below).
5. Returns the validated body, or falls back to the deterministic renderer.

If `preferred_provider` does not match any adapter id, the pipeline logs a config warning and falls back to the deterministic renderer rather than crashing.

## Render validation

```rust
pub fn validate_render(body: &str, facts: &Facts) -> ValidationResult {
    // (a) structure: expected section layout
    // (b) coverage:  >=80% of facts.tickets appear in body
    // (c) no-hallucination: body must not say "no work done" when facts or notes are non-empty
}
```

Where `ValidationResult` is roughly:

```rust
pub struct ValidationResult {
    pub ok: bool,
    pub coverage_pct: f32,
    pub missing_tickets: Vec<TicketId>,
    pub issues: Vec<String>,
}
```

On `!ok` the pipeline:

1. Logs the validation `issues` and the provider that produced the failing body.
2. Discards the LLM body.
3. Renders via the deterministic (template) renderer.
4. Writes the fallback event to the audit sidecar (`render.fallback = true`, `render.reason = issues.join("; ")`).

This means a hallucinating or structurally-broken LLM render never reaches the user's standup file; the deterministic render is the floor under the AI render.