# OpenAI Codex CLI / OpenAI API Adapter

This adapter covers two distinct things, both supported, both surfaced in the UI:

1. **Codex CLI** — OpenAI's 2025 agentic terminal CLI. Can run on a ChatGPT Plus/Pro subscription (credits) **or** on an OpenAI API key. Binary: `codex`.
2. **OpenAI API** — the standard OpenAI chat completions HTTP API for GPT and reasoning models. Used directly with a Bearer key.

Both paths produce prose standups; the user picks which via the mode toggle.

## CLI mode (Codex CLI)

**Command:**

```
codex exec --ephemeral --sandbox read-only --skip-git-repo-check --ignore-rules --model <model> -
```

The prompt is sent on stdin (`-`) so large fact sets do not hit `ARG_MAX`.
The render is ephemeral, read-only, non-interactive, and allowed outside a Git
repository. When the configured model is blank, `--model` is omitted so the
signed-in Codex account can select a compatible default. stdout is captured.

**Binary:** `codex`

**Discovery paths** (in order):

1. `ProviderConfig.cli_path` override.
2. `which codex` on `PATH`.
3. `~/.codex/bin/codex`
4. `~/.npm-global/bin/codex`
5. `/opt/homebrew/bin/codex`

Discovery runs `codex --version` to populate `CliInfo.version`.

### CLI auth

The Codex CLI manages its own auth at `~/.codex/auth.json`, populated by `codex login` (OAuth against ChatGPT account, subscription-credit based) **or** by writing an API key there. autostand does not manage this file; it only reports "auth file present / absent" in the Settings UI.

### Codex CLI config

The CLI reads `~/.codex/config.toml` for compatible model and reasoning defaults.
An explicitly configured model is passed through. The adapter always forces a
read-only sandbox for the render.

## API mode

**Endpoint:** `POST https://api.openai.com/v1/chat/completions`

**Headers:**

```
Authorization: Bearer <api_key>
content-type: application/json
```

**Body:**

```json
{
  "model": "<model>",
  "messages": [
    { "role": "system", "content": "<render-prompt>" },
    { "role": "user",   "content": "<prompt>" }
  ],
  "max_tokens": 4096
}
```

**API key:** stored in the OS keychain (service `autostand`, account `openai`). Never written to `config.json`. Never logged.

## Models

| Path | Models                                       |
|------|----------------------------------------------|
| CLI  | The signed-in Codex account's configured default, or an explicitly selected model |
| API  | `gpt-5`, `gpt-4o`, `o4-mini`                 |

All configurable. Default:

- CLI mode: leave blank to use the compatible default selected by Codex; an explicit model is passed through
- API mode: `gpt-5`

Reasoning settings for CLI mode remain owned by the user's `~/.codex/config.toml`.

## Usage reporting

Settings queries the installed Codex CLI through an isolated `codex app-server --stdio` process. After JSON-RPC initialization it calls `account/rateLimits/read` and parses only the documented rate-limit fields. `usedPercent`, window duration, and reset epoch are converted into `UsageWindow` values; exact remaining percentage is derived as `100 - usedPercent`.

Autostand never reads or copies `~/.codex/auth.json` during this probe. Missing CLI, unavailable rate limits, protocol errors, or timeouts produce an honest `unknown` health result instead of a synthetic percentage. API-request token usage and ChatGPT subscription quota are not conflated.

## Anti-recursion

`AUTOSTAND_RENDER=1` is set on the `codex` subprocess. (Codex CLI may emit its own session-end telemetry; the guard prevents re-entry into autostand's hook.)

## Timeout

Default **180s**. Configurable via `ProviderConfig.timeout_secs`.

## Rust struct

```rust
pub struct OpenAiAdapter {
    cli_path: Option<PathBuf>,
    api_key: Option<String>,   // lazily loaded from keychain
}

#[async_trait]
impl LlmAdapter for OpenAiAdapter {
    fn id(&self) -> &str { "openai" }
    fn display_name(&self) -> &str { "OpenAI (Codex CLI / API)" }
    async fn detect_cli(&self) -> Option<CliInfo> { /* codex discovery */ }
    async fn has_api_key(&self) -> bool { /* keyring lookup, account "openai" */ }
    async fn render(&self, prompt: &str, system_prompt: &str, config: &ProviderConfig) -> Result<RenderOutput, LlmError> { /* ... */ }
    async fn test_connection(&self, config: &ProviderConfig) -> Result<TestResult, LlmError> { /* ... */ }
}
```

## Config fields

| Field              | Type            | Default        | Notes                                   |
|--------------------|-----------------|----------------|-----------------------------------------|
| `model`            | `String`        | CLI: account default, API: `gpt-5` | Blank omits `--model` for CLI and resolves to `gpt-5` for API. |
| `mode`             | `ProviderMode`  | `CliFirst`     | CliFirst / CliOnly / ApiOnly / ApiFallback. |
| `timeout_secs`     | `u64`           | `180`          |                                         |
| `cli_path`         | `Option<PathBuf>` | `None`       | Manual `codex` binary override.         |
| `api_key`          | keychain        | `None`         | account `openai`.                       |

## Settings UI

Two clearly separated sections:

### Codex CLI section
- **CLI detected** — green check + path + `codex --version`. Red X if not found.
- **Auth status** — reads `~/.codex/auth.json`: "Logged in via ChatGPT (subscription)" / "API key present" / "Not authenticated — run `codex login`".
- **Model** — discovered-model Select with a custom value option; blank uses the signed-in account's compatible default.

### OpenAI API section
- **API key status** — "Stored in keychain" / "Not set". "Set key" button.
- **Model** — dropdown: `gpt-5` / `gpt-4o` / `o4-mini`.

### Common
- **Mode** — segmented control: CliFirst / CliOnly / ApiOnly / ApiFallback.
- **Timeout** — numeric field (seconds).
- **Test** — button: runs `test_connection()` and reports `TestResult.message` + latency.
