# Codex CLI (`codex`)

The Codex CLI source reads OpenAI's Codex CLI session transcripts. It produces a conversation digest (user-typed prompts) and an edited-file attribution list (file paths from `function_call` blocks, attributed to repos under `GITHUB_DIR`).

## What it provides

1. **Session history** — user-typed prompts from Codex sessions in the window.
2. **Edited file attribution** — file paths from `function_call` blocks, attributed to repos by path-prefix match.

Emitted as:
```
SourceData {
    facts: None,
    notes: None,
    enrichment: Some(conv_digest),   // CONTEXT block
    files: Some(edited_files),       // files-edited per-repo list
}
```

## Paths read (read-only, under `~/.codex/`)

- **`~/.codex/sessions/YYYY/MM/DD/rollout-{ISO-ts}-{uuid}.jsonl`** — JSONL session files. One JSON object per line. Codex writes one file per session under a year/month/day hierarchy.
- **`~/.codex/history.jsonl`** — global index of sessions (`session_id` + `timestamp` + `text`). The adapter uses this to find sessions whose timestamp falls in the window quickly, then opens the matching `rollout-*.jsonl` for full detail.
- **`~/.codex/config.toml`** — model, reasoning effort, project trust levels. **Not read for data**; the adapter only checks for its existence to confirm Codex is installed.

The adapter never writes under `~/.codex`. All reads are read-only.

### JSONL object types observed in rollout files

| `type` | Role |
|---|---|
| `event_msg` | Event message (may carry user content). |
| `user_message` | User-typed prompt. |
| `response_item` | Assistant `message` item. |
| `function_call` | Tool/function call (carries path arguments). |
| `function_call_output` | Tool result body. **Never read.** |
| `reasoning` | Chain-of-thought reasoning. **Never read.** |
| `ghost_snapshot` | Internal snapshot. |
| `turn_context` | Turn metadata. |
| `token_count` | Token accounting. |
| `session_meta` | Session metadata. |

## JSONL parsing rules

### User prompts
- Lines where `type == "user_message"`, or `event_msg` objects whose payload carries user content.
- Extract text and apply the **same filters as Claude Code**: drop slash commands, XML wrappers, "you are" meta, code pastes, and `standup_meta` matches.
- Max 15 deduped snippets, ≤ 200 chars each.

### Edited files
- Lines where `type == "function_call"`.
- Extract path arguments from the tool call payload (field names probed defensively — `file_path`, `path`, `arguments.file_path`, etc.).
- Attribute to repos by path-prefix match against `GITHUB_DIR`.
- Only edit-class tools counted (Edit, Write, Create, etc.).

### NEVER read
- `function_call_output` bodies (may contain file contents / diffs / secrets).
- `reasoning` blocks (chain-of-thought — never persisted by autostand).
- Any sensitive content field not explicitly whitelisted.

## Auth

None. The session files are local and unauthenticated. (Codex CLI itself uses an API key stored in its own config; autostand does not touch it.)

## Config fields

| Field | Type | Default | Purpose |
|---|---|---|---|
| `codex_enabled` | `bool` | false | Master toggle. **Opt-in** — Codex is newer. |

## Rust adapter

```rust
pub struct CodexDataSource;

#[async_trait]
impl DataSource for CodexDataSource {
    fn id(&self) -> &str { "codex" }
    fn display_name(&self) -> &str { "Codex CLI" }
    fn is_available(&self) -> bool {
        dirs::home_dir().map(|h| h.join(".codex").exists()).unwrap_or(false)
    }
    async fn gather(&self, window: &DateWindow, config: &AppConfig)
        -> Result<SourceData, DataSourceError> {
        // 1. read ~/.codex/history.jsonl to find in-window sessions
        // 2. for each, open the matching rollout-*.jsonl
        // 3. route lines by type; collect prompts + edited files
        Ok(SourceData {
            facts: None,
            notes: None,
            enrichment: Some(conv_digest),
            files: Some(edited_files),
        })
    }
}
```

## Output format

Same as Claude Code:

### CONTEXT block (enrichment)
```
## CONTEXT
prompts:
- snippet 1 (<=200 chars)
- snippet 2
- ...
```

### files-edited (files)
Per-repo list of edited file basenames:
```
### files-edited: main.rs, lib.rs
```

## Cache

- TTL: **2700 seconds** (45 minutes).
- Cache key includes the `DateWindow` hash and source id `codex`.
- Cached payload: both `enrichment` and `files`.