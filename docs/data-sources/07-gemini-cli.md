# Gemini CLI (`gemini-cli`)

The Gemini CLI source reads Google's Gemini CLI session transcripts. It produces a conversation digest (user-typed prompts) and an edited-file attribution list (file paths from tool calls, attributed to repos under `GITHUB_DIR`).

## What it provides

1. **Session history** — user-typed prompts from Gemini CLI sessions in the window.
2. **Edited file attribution** — file paths from tool-call blocks, attributed to repos by path-prefix match.

Emitted as:
```
SourceData {
    facts: None,
    notes: None,
    enrichment: Some(conv_digest),   // CONTEXT block
    files: Some(edited_files),       // files-edited per-repo list
}
```

## Paths read (read-only, under `~/.gemini/`)

- Sessions are persisted as **JSONL streams** via Gemini CLI's `chatRecordingService.ts`. Each session is identified by a `sessionId` (UUID).
- **Exact subpath under `~/.gemini/` is to be confirmed during implementation.** Likely candidates are `~/.gemini/sessions/` or `~/.gemini/history/`.
- The adapter does **not** hard-code the subpath. Instead it **scans `~/.gemini/` recursively** for `.jsonl` session files and parses each defensively.
- Gemini CLI also uses an internal **shadow git** for session checkpointing; autostand does not read that git repo (it reads only the JSONL streams).

## JSONL parsing rules

The exact JSONL schema is not yet fully documented upstream. The adapter is written **defensively**: it probes each JSON line for common field names and routes accordingly.

Probed field names:
- `type` / `role` — to identify user vs assistant vs tool lines.
- `content` / `text` — to extract user-typed text.
- `tool_use` / `file_path` / `path` — to identify file edits.

### User prompts
- Lines identified as user-typed (by `role == "user"` or an equivalent probed field).
- Extract text and apply the **same filters as Claude Code / Codex**: drop slash commands, XML wrappers, "you are" meta, code pastes, and `standup_meta` matches.
- Max 15 deduped snippets, ≤ 200 chars each.

### Edited files
- Lines identified as tool calls.
- Extract path keys (`file_path`, `path`, etc.).
- Attribute to repos by path-prefix match against `GITHUB_DIR`.
- Only edit-class tools counted.

### NEVER read
- Tool result/output bodies.
- Any content block not explicitly whitelisted.

## Auth

None for the session files (local JSONL). Gemini CLI itself uses Google OAuth stored at `~/.gemini/`; autostand does not touch the OAuth state.

## Config fields

| Field | Type | Default | Purpose |
|---|---|---|---|
| `gemini_cli_enabled` | `bool` | false | Master toggle. **Opt-in.** |

## Rust adapter

```rust
pub struct GeminiCliDataSource;

#[async_trait]
impl DataSource for GeminiCliDataSource {
    fn id(&self) -> &str { "gemini-cli" }
    fn display_name(&self) -> &str { "Gemini CLI" }
    fn is_available(&self) -> bool {
        dirs::home_dir().map(|h| h.join(".gemini").exists()).unwrap_or(false)
    }
    async fn gather(&self, window: &DateWindow, config: &AppConfig)
        -> Result<SourceData, DataSourceError> {
        // 1. recursively scan ~/.gemini/ for *.jsonl
        // 2. parse each defensively, probing for common field names
        // 3. filter by mtime / embedded timestamp against the window
        // 4. collect <=15 deduped prompts + edited file paths
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
- Cache key includes the `DateWindow` hash and source id `gemini-cli`.
- Cached payload: both `enrichment` and `files`.

## Note

> Gemini CLI session storage format is not yet fully documented. The adapter probes `~/.gemini/` for JSONL files and parses defensively. Report format changes to update the adapter.