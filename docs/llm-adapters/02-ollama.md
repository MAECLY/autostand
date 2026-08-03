# Ollama Adapter

Ollama runs models locally, so it is the privacy-friendly provider: no data leaves the machine. It is exposed both as a CLI (`ollama run`) and an HTTP API on localhost.

## CLI mode

**Command:**

```
ollama run <model>
```

- The prompt is piped to the process's **stdin** (not a positional arg), then stdin is closed. `ollama run` reads the prompt, emits the completion on stdout, and exits when stdin hits EOF.
- `--model <model>` selects the model (must already be pulled via `ollama pull <model>`).

**Binary:** `ollama`

**Discovery paths** (in order):

1. `ProviderConfig.cli_path` override.
2. `which ollama` on `PATH`.
3. `/usr/local/bin/ollama`
4. `/opt/homebrew/bin/ollama`
5. `~/.ollama/bin/ollama` (used by the official macOS installer)

Discovery runs `ollama --version` to populate `CliInfo.version`.

## Auth

- **CLI auth:** none. Ollama runs locally and has no authentication.
- **API auth (local):** none. `http://localhost:11434` is open.
- **API auth (remote / Ollama Cloud / custom base):** optional API key, sent as `Authorization: Bearer <key>`. If the user configures a remote base URL and a key, the key is stored in the OS keychain (service `autostand`, account `ollama`). Local requests never include the header.

## API mode

**Primary endpoint (Ollama native):**

`POST <base_url>/api/chat` — default `http://localhost:11434/api/chat`

**Body:**

```json
{
  "model": "<model>",
  "messages": [
    { "role": "user", "content": "<prompt>" }
  ],
  "stream": false
}
```

The system prompt is sent as a `system` field at the top level (or as a `role: "system"` message — the adapter sends it as the `system` field, which Ollama accepts).

**OpenAI-compatible endpoint:**

`POST <base_url>/v1/chat/completions` — used if `ProviderConfig.use_openai_compat` is true (some users prefer the OpenAI shape, e.g. when proxying through an OpenAI-compatible gateway). Body shape matches the OpenAI chat completions schema.

The adapter picks native vs. OpenAI-compat based on `ProviderConfig.use_openai_compat` (default: native).

## Models

Models are user-managed — the user pulls them via `ollama pull <model>` before configuring them here. The adapter does not pull models.

Common choices:

- `llama3.2` (default)
- `qwen2.5`
- `deepseek-r1`
- `mistral`
- `gemma2`

Default: `llama3.2`.

The Settings UI has a "List available" button that calls `GET <base_url>/api/tags` and shows the installed models, so the user does not have to remember model names.

## Timeout

Default **300s** — local models, especially on first inference when the model needs to load into VRAM, can be slow. Configurable via `ProviderConfig.timeout_secs`.

## Rust struct

```rust
pub struct OllamaAdapter {
    cli_path: Option<PathBuf>,
    api_base_url: Option<String>,   // default http://localhost:11434
    api_key: Option<String>,        // only for remote; lazily loaded from keychain
}

#[async_trait]
impl LlmAdapter for OllamaAdapter {
    fn id(&self) -> &str { "ollama" }
    fn display_name(&self) -> &str { "Ollama (local)" }
    async fn detect_cli(&self) -> Option<CliInfo> { /* ... */ }
    async fn has_api_key(&self) -> bool { /* true only if a remote key is set */ }
    async fn render(&self, prompt: &str, system_prompt: &str, config: &ProviderConfig) -> Result<RenderOutput, LlmError> { /* ... */ }
    async fn test_connection(&self, config: &ProviderConfig) -> Result<TestResult, LlmError> { /* ping /api/tags */ }
}
```

## Anti-recursion

`AUTOSTAND_RENDER=1` is set on the `ollama run` subprocess, even though Ollama itself does not emit session-end events — it is harmless and keeps the env contract uniform across providers.

## Config fields

| Field              | Type            | Default                    | Notes                              |
|--------------------|-----------------|----------------------------|------------------------------------|
| `model`            | `String`        | `"llama3.2"`               | Must be pulled first.              |
| `mode`             | `ProviderMode`  | `CliFirst`                 |                                    |
| `timeout_secs`     | `u64`           | `300`                      | Local models can be slow.          |
| `api_base_url`     | `String`        | `http://localhost:11434`   | Override for remote/Ollama Cloud.  |
| `use_openai_compat`| `bool`          | `false`                    | Use `/v1/chat/completions` shape.  |
| `api_key`          | keychain        | `None`                     | Only for remote.                   |
| `cli_path`         | `Option<PathBuf>` | `None`                   | Manual binary override.            |

## Settings UI

- **Base URL** — text input, prefilled `http://localhost:11434`.
- **Model** — text input + "List available" button (`GET /api/tags`).
- **Mode** — segmented control.
- **Timeout** — numeric field (seconds).
- **OpenAI-compat toggle** — switch.
- **API key** — only shown if base URL is non-localhost. "Set key" / "Clear key" via keychain.
- **Test** — button. For local, pings `/api/tags`; for remote, does a tiny `render` with a throwaway prompt.