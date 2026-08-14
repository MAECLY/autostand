# Claude (Anthropic) Adapter

Anthropic's Claude is the default provider for `autostand`. It is available via the `claude` CLI (Claude Code) or the Anthropic Messages API.

## CLI mode

**Command:**

```
claude -p --no-session-persistence --model <model> < prompt.txt
```

- `-p` / `--print` — non-interactive "print" mode (exit after producing output, no REPL).
- `--model <model>` — overrides the model the CLI uses.
- The full render prompt is sent on stdin, and `--no-session-persistence`
  prevents the synthetic render from being gathered into a future standup.

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

Claude CLI manages its own auth. On macOS the live session lives in the keychain item `Claude Code-credentials`; older installs and non-macOS hosts use `~/.claude/.credentials.json` (or `$CLAUDE_CONFIG_DIR/.credentials.json`). autostand **reads** these, **read-only**, and never writes, refreshes, rotates, or deletes them — see [Usage reporting](#usage-reporting). If the CLI is authenticated, CLI-mode rendering just works.

## Usage reporting

> **Policy change.** This section supersedes the previous stance that Claude usage is reported as `unknown`. See [`../specs/provider-usage.md`](../specs/provider-usage.md) for the full contract and the decisions behind it.

Autostand still does not alter the user's statusline configuration and does not scrape interactive output. Instead, `autostand-adapters::usage::claude` reads the credential Claude Code already stored and calls Anthropic's own usage endpoint:

```
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <accessToken>
Accept: application/json
Content-Type: application/json
anthropic-beta: oauth-2025-04-20
User-Agent: claude-code/<CLAUDE_CODE_VERSION>
```

No `anthropic-version` header — the vendor's own client omits it here. `CLAUDE_CODE_VERSION` is a single constant in `usage/claude/client.rs`, so the identity is bumped in one place.

**Credential order** (read-only; the keychain always beats the file, and the loop advances to the next candidate only on an expiry-class rejection):

1. macOS keychain, service `Claude Code-credentials` — plus `Claude Code-credentials-<sha256(CLAUDE_CONFIG_DIR)[..8]>` first when `CLAUDE_CONFIG_DIR` is set. Per service: the current user's item (`-a $USER`), then the legacy service-only item. A keychain read happens only on a **manual** refresh, so a background pass never raises a macOS dialog.
2. `$CLAUDE_CONFIG_DIR/.credentials.json`, else `~/.claude/.credentials.json`. Plain or hex-encoded JSON.
3. `CLAUDE_CODE_OAUTH_TOKEN` — **last**. A `claude setup-token` value can run inference but cannot read subscription limits, so it reports `usage_requires_cli_login` rather than shadowing a real login.

**Windows reported:** `five_hour` → `session` (5h), `seven_day` → `weekly` (7d), `seven_day_sonnet` → `sonnet` (7d), one row per `limits[]` entry with `kind == "weekly_scoped"` labelled from `scope.model.display_name`, and `extra_usage` in USD (a monthly cap makes it a meter; without one it is an open-ended balance). The plan string (`"Max 20x"`) comes from the credential's `subscriptionType` plus the `\d+x` multiplier in `rateLimitTier`.

**Scope gate.** Reading usage requires the `user:profile` scope. A login without it is not an error: availability stays normal with the notice `Re-login for live usage`, because inference still works and only the meters are missing.

**429 cooldown.** `Retry-After` (integer seconds or HTTP date; 5 minutes when absent) starts a cooldown during which the endpoint is **not called at all** — including on a manual refresh. The last good reading is served with `stale: true` and a notice. The cooldown is keyed by `sha256(accessToken)`, so signing into a different account starts clean.

**Read-only consequence.** An expired token surfaces as `auth_required`; autostand never calls Anthropic's refresh endpoint. Running `claude` once clears it. As before, an inferred exhausted/auth/rate-limit state never includes a fabricated percentage or reset time — a field the provider did not send is `None`, never `0%`.

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
