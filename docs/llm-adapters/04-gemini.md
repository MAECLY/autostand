# Gemini (Google) Adapter

Google's Gemini is supported both as a CLI (the Gemini CLI by Google) and via the Generative Language API.

## CLI mode

**Command:**

```
gemini -p "<prompt>"
```

- `-p` / `--prompt` — non-interactive prompt mode (emit and exit).
- The full render prompt is passed as the positional argument.

**Binary:** `gemini`

**Discovery paths** (in order):

1. `ProviderConfig.cli_path` override.
2. `which gemini` on `PATH`.
3. `~/.gemini/bin/gemini`
4. `/usr/local/bin/gemini`
5. npm global bin (e.g. `~/.npm-global/bin/gemini`) — installed via `npm i -g @google/gemini-cli` (or the equivalent Google-published package; the discovery is by binary name, not package).

Discovery runs `gemini --version` to populate `CliInfo.version`.

## CLI auth

- **Google OAuth** — `gemini auth login` opens a browser flow and stores the token under `~/.gemini/`.
- **API key** — alternatively set `GEMINI_API_KEY` in the environment; the CLI picks it up.

autostand does **not** write to `~/.gemini/` or inject `GEMINI_API_KEY`; it relies on whichever the user has configured for the CLI.

## API mode

**Endpoint:**

```
POST https://generativelanguage.googleapis.com/v1beta/models/<model>:generateContent?key=<api_key>
```

The API key is passed as the `key` query parameter (Gemini's auth convention).

**Headers:**

```
content-type: application/json
```

**Body:**

```json
{
  "contents": [
    { "parts": [ { "text": "<prompt>" } ] }
  ],
  "systemInstruction": {
    "parts": [ { "text": "<render-prompt>" } ]
  }
}
```

The render prompt goes into `systemInstruction.parts[].text`; the user prompt goes into `contents[].parts[].text`.

**API key:** stored in the OS keychain (service `autostand`, account `gemini`). Never written to `config.json`. Never logged.

## Models

- `gemini-2.5-pro`
- `gemini-2.5-flash` (default — best speed/quality tradeoff)
- `gemini-2.0-flash`
- `gemini-2.5-flash-lite` (optional, fastest)

Default: `gemini-2.5-flash`.

## Anti-recursion

`AUTOSTAND_RENDER=1` is set on the `gemini` subprocess. Gemini CLI does not emit autostand-style session-end hooks today, but the env var is harmless and keeps the contract uniform.

## Timeout

Default **180s**. Configurable via `ProviderConfig.timeout_secs`. CLI uses `tokio::time::timeout`; API uses `reqwest` with a matching timeout.

## Rust struct

```rust
pub struct GeminiAdapter {
    cli_path: Option<PathBuf>,
    api_key: Option<String>,   // lazily loaded from keychain
}

#[async_trait]
impl LlmAdapter for GeminiAdapter {
    fn id(&self) -> &str { "gemini" }
    fn display_name(&self) -> &str { "Gemini (Google)" }
    async fn detect_cli(&self) -> Option<CliInfo> { /* ... */ }
    async fn has_api_key(&self) -> bool { /* keyring lookup, account "gemini" */ }
    async fn render(&self, prompt: &str, system_prompt: &str, config: &ProviderConfig) -> Result<RenderOutput, LlmError> { /* ... */ }
    async fn test_connection(&self, config: &ProviderConfig) -> Result<TestResult, LlmError> { /* ... */ }
}
```

## Config fields

| Field           | Type            | Default              | Notes                              |
|-----------------|-----------------|----------------------|------------------------------------|
| `model`         | `String`        | `"gemini-2.5-flash"` |                                    |
| `mode`          | `ProviderMode`  | `CliFirst`           |                                    |
| `timeout_secs`  | `u64`           | `180`                |                                    |
| `cli_path`      | `Option<PathBuf>` | `None`            | Manual binary override.            |
| `api_key`       | keychain        | `None`               | account `gemini`.                  |

## Settings UI

- **CLI detected** — green check + path + `gemini --version`. Red X if not found.
- **API key status** — "Stored in keychain" / "Not set". "Set key" button (keychain dialog).
- **Model** — dropdown: `gemini-2.5-pro` / `gemini-2.5-flash` / `gemini-2.0-flash`.
- **Mode** — segmented control: CliFirst / CliOnly / ApiOnly / ApiFallback.
- **Timeout** — numeric field (seconds).
- **Test** — button: runs `test_connection()` and reports `TestResult.message` + latency.