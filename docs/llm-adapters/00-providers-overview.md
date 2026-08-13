# LLM Adapters — Providers Overview

`autostand` supports multiple LLM providers for rendering daily standups from gathered activity data. Each cloud/CLI provider implements the Rust `LlmAdapter` contract. At render time the app walks the user's ordered, enabled provider chain until one transport returns a body that also passes standup validation.

## Supported providers

| # | Provider       | Vendor       | CLI binary   | API base                                              |
|---|----------------|--------------|--------------|-------------------------------------------------------|
| 1 | Claude         | Anthropic    | `claude`     | `https://api.anthropic.com/v1/messages`              |
| 2 | Ollama         | local / openai-compat | `ollama`     | `http://localhost:11434/api/chat` (or `/v1/chat/completions`) |
| 3 | OpenAI Codex CLI / OpenAI API | OpenAI       | `codex`      | `https://api.openai.com/v1/chat/completions`         |
| 4 | Gemini         | Google       | `gemini`     | `https://generativelanguage.googleapis.com/v1beta/models/<model>:generateContent` |
| 5 | Grok           | xAI          | `grok` / `grok-cli` | `https://api.x.ai/v1/chat/completions`              |
| 6 | Built-in Local AI | local / llama.cpp | `autostand-local-llm` + `llama-cli` | none |

## Access modes

Cloud/CLI adapters support four access modes, configurable per provider in Settings. `builtin-local` is always CLI-only:

- **`CliFirst`** — Try the local CLI first; fall back to the API key if the CLI is unavailable or fails. Default for most providers.
- **`CliOnly`** — Use the CLI exclusively. Render fails if the CLI binary is not found or exits non-zero. No API key is consulted.
- **`ApiOnly`** — Use the HTTP API exclusively with the key from the OS keychain. The CLI is never invoked.
- **`ApiFallback`** — Use the CLI when it is detected; otherwise use the API. Unlike `CliFirst`, a detected CLI failure does not proceed to API inside the same provider.

Mode selection lives in `ProviderConfig.mode` and is editable from Settings → LLM Providers.

## Ordered provider failover

Settings → Providers stores an explicit `llm.provider_order`, per-provider enablement, and `llm.fallback_enabled`. When no explicit order exists, migration-safe resolution starts with `preferred_provider` and appends providers in their stored order. Disabled providers are skipped; turning fallback off restricts rendering to the first resolved provider.

The CLI/API plan runs inside one provider before the chain advances. Authentication, quota/billing, missing CLI, unsupported model, timeout, transport, empty-body, and validation failures are isolated to that provider. A reported rate-limit delay is retried once when enabled and at or below the configured 30-second default ceiling. User cancellation is not represented as a provider failure.

`Auto` uses the deterministic renderer after the chain is exhausted. `Llm` is strict and returns a safe aggregate error. Every attempt contains only provider/channel/model/status/classifier/latency; raw provider output and credentials never enter the pipeline log or audit trail.

## Provider usage truthfulness

Settings distinguishes authoritative quota windows from inferred availability:

- OpenAI/Codex CLI is probed through `codex app-server --stdio` and the `account/rateLimits/read` JSON-RPC method. Reported primary/secondary windows become five-hour/weekly labels when their durations identify them.
- Claude Code and Grok consumer CLIs currently have no supported non-interactive quota contract used by Autostand. They remain `unknown` unless a real render produces a classified failure.
- A provider failure can produce `failure_inferred` states such as `exhausted`, `rate_limited`, `auth_required`, or `model_unavailable`, but never a fabricated percentage or reset time.

The IPC uses `ProviderHealth`, `UsageWindow`, `UsageSource`, and `ProviderAvailability`; see `docs/tauri/02-ipc-contracts.md`.

## Local options

Ollama remains a separately installed and user-managed local provider. Settings → Local AI additionally manages a curated GGUF catalog under Autostand's state directory. See `06-built-in-local.md` for the exact download, licensing, and runtime boundary.

## `LlmAdapter` trait

All providers implement a single Rust trait defined in `autostand-adapters`:

```rust
#[async_trait]
pub trait LlmAdapter: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    async fn detect_cli(&self) -> Option<CliInfo>;
    async fn has_api_key(&self) -> bool;
    async fn render(&self, prompt: &str, config: &ProviderConfig) -> Result<RenderOutput, LlmError>;
    async fn test_connection(&self, config: &ProviderConfig) -> Result<TestResult, LlmError>;
}

pub struct CliInfo { pub path: PathBuf, pub version: String }

pub struct RenderOutput {
    pub body: String,
    pub mode_used: RenderModeUsed,
    pub model: String,
    pub latency_ms: u64,
}

pub enum RenderModeUsed { Cli, Api }

pub enum LlmError {
    Timeout,
    CliNotFound,
    ApiError(String),
    AuthError,
    ParseError,
    RateLimit,
}

pub struct TestResult {
    pub ok: bool,
    pub message: String,
    pub latency_ms: u64,
}
```

See `adapter-trait.md` for the expanded form used by implementers.

## Anti-recursion

`autostand` installs a session-end hook that triggers standup generation when an AI coding session ends. Several AI CLIs (notably `claude`, `codex`, `gemini`) are themselves session producers, so if the adapter spawns one of them and that spawn re-triggers the session-end hook, you get an infinite render loop.

To break the loop, **every** CLI invocation sets the environment variable:

```
AUTOSTAND_RENDER=1
```

The app's own SessionEnd hook checks `AUTOSTAND_RENDER` and aborts re-entry if it is set. This applies to all providers and both CLI variants. Additional provider-specific guard env vars may also be set (e.g. Claude sets `CLAUDE_STANDUP_RENDER=1` for backward-compat with the original App Script).

## Render flow

Per `render()` call:

1. **Read config** — `config.mode`, `config.model`, `config.timeout_secs`, `config.cli_path` override.
2. **CLI path** (`CliFirst` / `CliOnly`):
   1. `detect_cli()` resolves the binary path (config override → PATH `which` → platform defaults). Returns `None` → in `CliFirst` we fall through to API; in `CliOnly` we return `LlmError::CliNotFound`.
   2. Spawn subprocess via `tokio::process::Command` with:
      - `AUTOSTAND_RENDER=1` (anti-recursion)
      - prompt on stdin **or** as a positional arg (provider-specific)
      - stdout/stderr piped
   3. Wrap the await in `tokio::time::timeout(timeout_secs, ...)`.
   4. Capture stdout, parse the prose body. On non-zero exit → `LlmError::CliExitError`.
3. **API path** (`ApiOnly`, or CLI failure in `CliFirst`):
   1. Load API key from the OS keychain (`keyring` crate, service `autostand`, account `<provider-id>`).
   2. `reqwest::Client` with `timeout(timeout_secs)`.
   3. POST the provider's chat endpoint with the system + user messages.
   4. Parse JSON response → extract assistant text. On 401/403 → `AuthError`; on 429 → `RateLimit`; other non-2xx → `ApiError`.
4. **Return** `RenderOutput { body, mode_used, model, latency_ms }`.

`mode_used` is recorded so the audit sidecar can tell whether the standup was produced locally or via cloud API.

## Timeout

- Default: **180s** per provider (300s for Ollama, since local first-inference can be slow).
- CLI: `tokio::time::timeout` around the `Command::wait_with_output()` future.
- API: `reqwest::ClientBuilder::timeout(Duration::from_secs(...))`.
- Configurable per provider via `ProviderConfig.timeout_secs`.

## Secrets handling

- **Prompt scrub** — before any render, the pipeline runs `redact_secrets(prompt)` which strips API keys, tokens, passwords, and other high-entropy secrets from the activity text. The scrubbed text is what reaches the CLI subprocess or API request body.
- **API keys** — stored in the OS keychain (macOS Keychain / Windows Credential Manager / libsecret on Linux), never in `config.json`, never written to logs or audit sidecars.
- **CLI auth** — the CLI's own credential store (`~/.claude/.credentials.json`, `~/.codex/auth.json`, `~/.gemini/`, etc.) is used as-is; autostand never reads or writes those files.

## Validation

After `render()` returns a body, the pipeline runs:

```rust
validate_render(body, facts) -> ValidationResult
```

which checks:

- **(a) Structure** — the body has the expected section layout (`**<repo> — [TICKET](url) — title**` blocks, optional `**PR Review**` tail).
- **(b) Coverage** — at least **80%** of the tickets present in the FACTS source appear in the rendered standup. Missing tickets are logged.
- **(c) No "no work done" hallucination** — if FACTS or NOTES contain content, the body must not claim "no work done" / "nothing to report".

If validation fails, the pipeline discards that provider's body, records `validation_failed`, and advances through the ordered provider chain. After the chain is exhausted, `Auto` uses the **deterministic renderer** (template-based, no LLM) while strict `Llm` returns an error. The attempt history is recorded in the audit sidecar.
