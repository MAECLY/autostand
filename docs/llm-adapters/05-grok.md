# Grok (xAI) Adapter

Grok is xAI's model family. The CLI landscape around Grok is fragmented, so the adapter is variant-aware: it auto-detects which Grok CLI is installed and adapts its invocation and auth model accordingly. The user can also pin a preferred variant in Settings.

## CLI variants

Three variants are supported:

### 1. Official xAI CLI — "Grok Build"

Powered by Grok 4.5 (and newer Grok models as they ship). xAI's first-party agentic terminal tool.

- **Binary:** `grok` or `grok-build` (the adapter tries `grok` first, then `grok-build`).
- **Discovery:** PATH + the xAI installer's default location (e.g. `~/.grok/bin/grok`).
- **Auth:** xAI OAuth via `grok login` (token stored under `~/.grok/`), **or** an xAI API key.

### 2. `superagent-ai/grok-cli`

Open-source, Bun-based, talks to the xAI API under the hood. Config-driven via `.env`.

- **Binary:** `grok-cli`
- **Discovery:** PATH + `/opt/homebrew/bin/grok-cli` + npm/Bun global bin.
- **Auth:** `.env` containing `XAI_API_KEY=...`. The user is expected to have created this; autostand reads no secret from `.env` itself.

### 3. `grokcli.dev`

Community-built, Claude-Code-style with a Plan Mode. Uses the xAI API.

- **Binary:** `grok-cli` (collides with superagent's binary name; disambiguation is done by `--version` output and `--help` text).
- **Discovery:** PATH only.
- **Auth:** API key (passed via env or config file per the tool's docs).

### Auto-detect

`detect_cli()` walks the variants in order (official → superagent → grokcli.dev), runs `<bin> --version` and inspects the output to fingerprint the variant, and returns a `CliInfo` annotated with the detected variant. If two variants are present, the user's `ProviderConfig.grok.preferred_variant` (`auto` by default) decides; `auto` picks official > superagent > grokcli.dev.

## CLI mode

Invocation depends on variant:

- Official (Grok Build TUI): `grok --prompt-file <tmp> --output-format plain --verbatim --max-turns 1 --permission-mode dontAsk`. A positional `grok "<prompt>"` starts the **interactive** TUI and hangs until timeout — never use that for a render.
- superagent: `grok-cli "<prompt>"`.
- grokcli.dev: `grok-cli "<prompt>"`.

The adapter picks the exact args per variant (some accept `--model`, some read it from `.env`). Official prompts go through a temp file so a large standup request cannot blow `ARG_MAX`; stdout is captured.

## API mode

**Endpoint:** `POST https://api.x.ai/v1/chat/completions`

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

**API key:** stored in the OS keychain (service `autostand`, account `grok`). Never written to `config.json`. Never logged.

## Models

- `grok-4.5` (default)
- `grok-4`
- `grok-3`
- `grok-code-fast-1` (where available)

Default: `grok-4.5`.

## Anti-recursion

`AUTOSTAND_RENDER=1` is set on whichever Grok binary is spawned. The grokcli.dev variant in particular emits Claude-Code-style session events; the guard prevents re-entry into autostand's hook.

## Timeout

Default **180s**. Configurable via `ProviderConfig.timeout_secs`.

## Rust struct

```rust
pub enum GrokVariant { Official, Superagent, GrokCliDev, Auto }

pub struct GrokAdapter {
    cli_path: Option<PathBuf>,
    variant: GrokVariant,
    api_key: Option<String>,   // lazily loaded from keychain
}

#[async_trait]
impl LlmAdapter for GrokAdapter {
    fn id(&self) -> &str { "grok" }
    fn display_name(&self) -> &str { "Grok (xAI)" }
    async fn detect_cli(&self) -> Option<CliInfo> { /* variant fingerprint */ }
    async fn has_api_key(&self) -> bool { /* keyring lookup, account "grok" */ }
    async fn render(&self, prompt: &str, system_prompt: &str, config: &ProviderConfig) -> Result<RenderOutput, LlmError> { /* ... */ }
    async fn test_connection(&self, config: &ProviderConfig) -> Result<TestResult, LlmError> { /* ... */ }
}
```

## Config fields

| Field               | Type            | Default   | Notes                                            |
|---------------------|-----------------|-----------|--------------------------------------------------|
| `model`             | `String`        | `"grok-4.5"` |                                                |
| `mode`              | `ProviderMode`  | `CliFirst`|                                                  |
| `timeout_secs`      | `u64`           | `180`     |                                                  |
| `preferred_variant` | `GrokVariant`   | `Auto`    | auto / official / superagent / grokcli.          |
| `cli_path`          | `Option<PathBuf>` | `None`  | Manual binary override (bypasses auto-detect).   |
| `api_key`           | keychain        | `None`    | account `grok`.                                  |

## Settings UI

- **Variant selector** — dropdown: auto / official (Grok Build) / superagent / grokcli.dev.
- **CLI path override** — text field (advanced). Shown when variant ≠ auto or when auto-detect fails.
- **CLI detected** — green check + path + version + detected variant badge. Red X if none found.
- **API key** — "Stored in keychain" / "Not set". "Set key" button.
- **Model** — dropdown: `grok-4.5` / `grok-4` / `grok-3`.
- **Mode** — segmented control.
- **Timeout** — numeric field (seconds).
- **Test** — button.
- **Note:** "Grok CLI ecosystem is evolving; if your variant isn't detected, set the CLI path manually."