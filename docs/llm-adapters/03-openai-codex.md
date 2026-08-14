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

Settings reads the account's quota over HTTP, through the `openai` usage probe in
`crates/autostand-adapters/src/usage/codex/`. This replaced the `codex app-server --stdio` spawn: no
child process, no eight-second protocol handshake, and the reading works whether or not the `codex`
CLI is on `PATH`.

**Credentials — read-only.** `$CODEX_HOME/auth.json` when that variable is set (the CLI's own rule:
the defaults are then not consulted at all), otherwise `~/.config/codex/auth.json` then
`~/.codex/auth.json`; the macOS keychain service `Codex Auth` is the fallback, and it is read only on
a refresh the user asked for. Plain or hex-encoded JSON is accepted. Autostand **never writes,
refreshes or rotates** this credential: an access token within 300s of its JWT `exp` is reported as
`session_expired`, not renewed. An `auth.json` carrying only `OPENAI_API_KEY` reports the typed
reason `usage_requires_cli_login` — an API key can run inference but cannot see subscription quota.

**Request.** `GET https://chatgpt.com/backend-api/wham/usage`, 10s timeout, with
`Authorization: Bearer <access_token>`, `Accept: application/json`, autostand's own `User-Agent`, and
`ChatGPT-Account-Id` when the credential names an account.

**Mapping.** Windows are classified by **duration**, not slot position: `limit_window_seconds == 18000`
is `session`, `== 604800` is `weekly`. The historical `primary`/`secondary` order is a fallback only
for a window whose duration is absent or unfamiliar — the vendor sometimes drops one limit and
promotes the weekly window into the primary slot, and reading the slot alone would label a 7-day
quota "Session". `credits.balance` becomes the `credits` balance resource, and `plan_type` maps
`prolite → Pro 5x`, `pro → Pro 20x`, otherwise title-case over `_`. Response headers
`x-codex-primary-used-percent`, `x-codex-secondary-used-percent` and `x-codex-credits-balance` fill
values the body omits, and the snapshot then reports `UsageSource::ResponseHeaders`; the body always
wins where both are present, because a header can be a stale echo.

Claiming rate-limit **reset credits** is deliberately out of scope: it is an irreversible account
mutation and belongs nowhere near a usage panel.

Tokens, headers and response bodies never reach a log, an error or a DTO. A failure carries only a
reason code (`not_logged_in`, `session_expired`, `usage_requires_cli_login`,
`credential_store_unavailable`, `rate_limited`, `network`, `timeout`, `unsupported_payload`,
`unexpected_status`), and a payload that no longer carries the documented fields degrades to "no
data" rather than a synthetic percentage. API-request token usage and ChatGPT subscription quota are
not conflated.

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
