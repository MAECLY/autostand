# OpenAI Codex CLI / OpenAI API Adapter

This adapter covers two distinct things, both supported, both surfaced in the UI:

1. **Codex CLI** — OpenAI's 2025 agentic terminal CLI. Can run on a ChatGPT Plus/Pro subscription (credits) **or** on an OpenAI API key. Binary: `codex`.
2. **OpenAI API** — the standard OpenAI chat completions HTTP API for GPT and reasoning models. Used directly with a Bearer key.

Both paths produce prose standups; the user picks which via the mode toggle.

## CLI mode (Codex CLI)

**Command:**

```
codex --model <model> "<prompt>"
```

or the explicit non-interactive form:

```
codex exec "<prompt>"
```

The adapter uses `codex exec "<prompt>"` with `--model` override. Prompt is passed as a positional arg. stdout is captured.

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

The CLI reads `~/.codex/config.toml` for defaults (model, reasoning effort, sandbox, etc.). The adapter passes `--model` to override the model and `--sandbox` `none` (or equivalent flag) to ensure the render is read-only and non-interactive. Other config.toml settings are respected.

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
| CLI  | `gpt-5-codex`, `o4-mini`, `gpt-5`            |
| API  | `gpt-5`, `gpt-4o`, `o4-mini`                 |

All configurable. Default:

- CLI mode: `gpt-5-codex`
- API mode: `gpt-5`

Reasoning models (`o4-mini`, `gpt-5`) accept a `reasoning_effort` setting on the CLI side; the adapter leaves that to `~/.codex/config.toml` unless the user overrides via `ProviderConfig.cli_extra_args`.

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
| `model`            | `String`        | CLI: `gpt-5-codex`, API: `gpt-5` | Resolved based on mode used. |
| `mode`             | `ProviderMode`  | `CliFirst`     | CliFirst / CliOnly / ApiOnly / ApiFallback. |
| `timeout_secs`     | `u64`           | `180`          |                                         |
| `cli_extra_args`   | `Vec<String>`   | `[]`           | Extra args for `codex exec`.             |
| `cli_path`         | `Option<PathBuf>` | `None`       | Manual `codex` binary override.         |
| `api_key`          | keychain        | `None`         | account `openai`.                       |

## Settings UI

Two clearly separated sections:

### Codex CLI section
- **CLI detected** — green check + path + `codex --version`. Red X if not found.
- **Auth status** — reads `~/.codex/auth.json`: "Logged in via ChatGPT (subscription)" / "API key present" / "Not authenticated — run `codex login`".
- **Model** — dropdown: `gpt-5-codex` / `o4-mini` / `gpt-5`.
- **Extra args** — optional text field (advanced).

### OpenAI API section
- **API key status** — "Stored in keychain" / "Not set". "Set key" button.
- **Model** — dropdown: `gpt-5` / `gpt-4o` / `o4-mini`.

### Common
- **Mode** — segmented control: CliFirst / CliOnly / ApiOnly / ApiFallback.
- **Timeout** — numeric field (seconds).
- **Test** — button: runs `test_connection()` and reports `TestResult.message` + latency.