# Claude (Anthropic) Adapter

Anthropic's Claude is the default provider for `autostand`. It is available via the `claude` CLI (Claude Code) or the Anthropic Messages API.

## CLI mode

**Command:**

```
claude -p --model <model> "<prompt>"
```

- `-p` / `--print` — non-interactive "print" mode (exit after producing output, no REPL).
- `--model <model>` — overrides the model the CLI uses.
- The full render prompt is passed as the trailing positional argument.

**Binary:** `claude` (the Anthropic CLI, distributed either via npm `@anthropic-ai/claude-code` or the native installer).

**Discovery paths** (in order):

1. `ProviderConfig.cli_path` override (if set in Settings).
2. `which claude` on `PATH`.
3. `~/.claude/local/claude`
4. `~/.npm-global/bin/claude`
5. `/opt/homebrew/bin/claude`
6. `/usr/local/bin/claude`

Discovery runs `<path> --version` to populate `CliInfo.version`.

## CLI auth

Claude CLI manages its own auth, stored at `~/.claude/.credentials.json` (OAuth token from `claude login` or an `ANTHROPIC_API_KEY` env var). autostand does **not** read, write, or manage this file. If the CLI is authenticated, CLI-mode rendering just works.

## API mode

**Endpoint:** `POST https://api.anthropic.com/v1/messages`

**Headers:**

```
x-api-key: <api_key>
anthropic-version: 2023-06-01
content-type: application/json
```

**Body:**

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 4096,
  "system": "<render-prompt>",
  "messages": [
    { "role": "user", "content": "<prompt>" }
  ]
}
```

**API key:** stored in the OS keychain via the `keyring` crate with:

- service: `autostand`
- account: `claude`

Never written to `config.json`. Never logged.

## Models

| Setting value | Actual model id   | Use                       |
|---------------|-------------------|---------------------------|
| `sonnet`      | `claude-sonnet-4-5` | Default — balanced quality and speed |
| `haiku`       | `claude-haiku-4-5` | Fast / cheap iterations    |
| `opus`        | `claude-opus-4-1`  | Premium, highest quality   |

Default: `sonnet`. The mapping from setting → model id lives in `ClaudeAdapter::resolve_model()`.

## Anti-recursion

When spawning `claude -p`, the adapter sets:

- `AUTOSTAND_RENDER=1` — the app's own session-end guard.
- `CLAUDE_STANDUP_RENDER=1` — backward-compat with the original App Script's `SessionEnd` hook, in case the user has both installed and the old hook is still active.

Both are cleared after the subprocess completes (they are scoped to the `Command` env, not `std::env`).

## Timeout

Default **180s**. Configurable via `ProviderConfig.timeout_secs`. CLI uses `tokio::time::timeout` around `Command::wait_with_output()`; API uses `reqwest` with a matching timeout.

## Rust struct

```rust
pub struct ClaudeAdapter {
    cli_path: Option<PathBuf>,
    api_key: Option<String>,   // lazily loaded from keychain
}

#[async_trait]
impl LlmAdapter for ClaudeAdapter {
    fn id(&self) -> &str { "claude" }
    fn display_name(&self) -> &str { "Claude (Anthropic)" }
    async fn detect_cli(&self) -> Option<CliInfo> { /* ... */ }
    async fn has_api_key(&self) -> bool { /* keyring lookup */ }
    async fn render(&self, prompt: &str, system_prompt: &str, config: &ProviderConfig) -> Result<RenderOutput, LlmError> { /* ... */ }
    async fn test_connection(&self, config: &ProviderConfig) -> Result<TestResult, LlmError> { /* ... */ }
}
```

## Config fields

| Field           | Type          | Default     | Notes                                  |
|-----------------|---------------|-------------|----------------------------------------|
| `model`         | `String`      | `"sonnet"`  | Resolved to a full model id at render. |
| `mode`          | `ProviderMode`| `CliFirst`  | CliFirst / CliOnly / ApiOnly / ApiFallback. |
| `timeout_secs`  | `u64`         | `180`       | Per-call timeout.                      |
| `cli_path`      | `Option<PathBuf>` | `None`  | Manual binary override.                |

## Settings UI

The Claude provider card in Settings shows:

- **CLI detected** — green check + binary path + `claude --version` output. Red X if not found.
- **API key status** — "Stored in keychain" / "Not set". "Set key" button (prompts via OS keychain dialog).
- **Model** — dropdown: sonnet / haiku / opus.
- **Mode** — segmented control: CliFirst / CliOnly / ApiOnly / ApiFallback.
- **Timeout** — numeric field (seconds).
- **Test** — button that runs `test_connection()`, shows `TestResult.message` and latency.