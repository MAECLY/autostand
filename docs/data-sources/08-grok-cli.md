# Grok CLI (`grok-cli`)

The Grok CLI source reads session transcripts from one of the several Grok CLI variants. It produces a conversation digest (user-typed prompts) and an edited-file attribution list (file paths from tool calls, attributed to repos under `GITHUB_DIR`).

## What it provides

1. **Session history** — user-typed prompts from Grok CLI sessions in the window.
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

## Paths read (read-only)

The Grok CLI ecosystem is **fragmented** — three variants exist, each with different storage locations:

| Variant | Origin | Likely storage |
|---|---|---|
| **Official** | `x.ai/cli` | `~/.grok/` (TBD — probe for sessions/history files) |
| **Superagent** | `superagent-ai/grok-cli` | `~/.config/grok-cli/` or `~/.grok-cli/` (Bun-based, `.env` config) |
| **GrokCliDev** | `grokcli.dev` | TBD |

The adapter **probes all candidate paths** in order:
1. `~/.grok/`
2. `~/.config/grok/`
3. `~/.config/grok-cli/`
4. `~/.grok-cli/`

For whichever path exists, it scans **recursively** for session/history files (JSONL or JSON) and parses each defensively.

## JSONL parsing rules

Same defensive pattern as Claude Code / Codex / Gemini CLI:

- Probe for common field names (`type`, `role`, `content`, `text`, `tool_use`, `file_path`, `path`).
- **User prompts**: lines identified as user-typed. Apply the same filters (slash commands, XML wrappers, "you are" meta, code pastes, `standup_meta`). Max 15 deduped snippets, ≤ 200 chars each.
- **Edited files**: lines identified as tool calls. Extract path keys, attribute to repos by path-prefix match. Only edit-class tools counted.
- **NEVER read**: tool result/output bodies, reasoning blocks, sensitive content.

## Auth

None for the session files (local). Grok CLI itself uses an xAI API key stored in `.env` or OAuth; autostand does not touch credentials.

## Config fields

| Field | Type | Default | Purpose |
|---|---|---|---|
| `grok_cli_enabled` | `bool` | false | Master toggle. **Opt-in.** |
| `grok_cli_variant` | enum `Auto`, `Official`, `Superagent`, `GrokCliDev` | `Auto` | Pin a variant, or let the adapter auto-detect. |

In `Auto` mode the adapter probes all candidate paths and infers the variant from which one exists. Pinning a variant skips probing and only reads that variant's path.

## Rust adapter

```rust
pub enum GrokCliVariant { Auto, Official, Superagent, GrokCliDev }

pub struct GrokCliDataSource {
    variant: GrokCliVariant,
}

#[async_trait]
impl DataSource for GrokCliDataSource {
    fn id(&self) -> &str { "grok-cli" }
    fn display_name(&self) -> &str { "Grok CLI" }
    fn is_available(&self) -> bool {
        // true if any candidate path exists (Auto) OR the pinned variant's path exists
    }
    async fn gather(&self, window: &DateWindow, config: &AppConfig)
        -> Result<SourceData, DataSourceError> {
        // 1. resolve candidate paths based on variant
        // 2. scan recursively for session/history files
        // 3. parse defensively, probing common field names
        // 4. filter by mtime / embedded timestamp against the window
        // 5. collect <=15 deduped prompts + edited file paths
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
- Cache key includes the `DateWindow` hash, source id `grok-cli`, **and** the resolved variant.
- Cached payload: both `enrichment` and `files`.

## Note

> Grok CLI ecosystem is evolving. The adapter auto-detects the variant and storage path. If your variant isn't found, report it to update the adapter.