# Claude Code sessions (`claude-code`)

The Claude Code source reads Anthropic's Claude Code CLI session transcripts and plan files. It produces two outputs: a conversation digest (plan titles + deduped user-typed prompt snippets) and an edited-file attribution list (file path basenames from tool-use blocks, attributed to repos under `GITHUB_DIR`).

## What it provides

1. **Conversation digest** — plan titles (from `~/.claude/plans/*.md` by mtime in the window) plus up to **15 deduped user-typed prompt snippets** (max 200 chars each) extracted from session transcripts.
2. **Edited file attribution** — file path **basenames** from `tool_use` blocks in assistant messages, attributed to repos by matching the full path prefix against `GITHUB_DIR` children.

Emitted as:
```
SourceData {
    facts: None,
    notes: None,
    enrichment: Some(conv_digest),   // CONTEXT block
    files: Some(edited_files),       // CLAUDE-FILES per-repo list
}
```

## Paths read (read-only, never write)

- `~/.claude/projects/*/*.jsonl` — session transcripts. Each file is JSONL: one JSON object per line.
- `~/.claude/plans/*.md` — plan files, filtered by mtime inside the `DateWindow`.

The adapter never creates, modifies, or deletes anything under `~/.claude`. All reads are buffered and read-only.

## JSONL parsing rules

Session transcripts are JSONL — one JSON object per line. The adapter parses each line independently and routes it by `type`.

### User prompts

- Lines where `type == "user"`.
- Extract text from `message.content` (string or array of text parts).
- **Filter out**:
  - Slash commands (text starts with `/`).
  - XML wrappers (`<...>`-wrapped payloads).
  - "you are" meta instructions (system-prompt-style preamble).
  - Code pastes (long code blocks — heuristic on line count / presence of fenced ``` blocks over a threshold).
  - Meta-work (matched by the `standup_meta` regex — see below).
- **Max 15 deduped snippets**, each truncated to **≤ 200 chars**. Dedup is by normalized text (whitespace-collapsed, case-folded).

### Edited files

- Lines where `type == "assistant"`.
- Inspect `message.content[]` entries where `type == "tool_use"`.
- Extract **only** these path keys: `file_path`, `notebook_path`, `path`.
- Attribute the extracted path to a repo by checking which `GITHUB_DIR` child is a prefix of the path.
- Only tools that **edit** files are counted: `Edit`, `Write`, `MultiEdit`, `NotebookEdit`, `Update`, `Create`. Read-only tools (`Read`, `Glob`, `Grep`, `LS`) are ignored.

### NEVER read

The adapter must never read or persist:

- `tool_result` blocks.
- `tool_use` content bodies beyond the path keys listed above.
- `old_string` / `new_string` fields (diff payloads — may contain secrets).
- Document, image, or attachment blocks.
- Any block whose content type is not explicitly whitelisted.

This is a hard secrets-safety boundary. Violations are treated as bugs.

## Auth

None. The session files are local and unauthenticated. (The LLM render step later uses the Claude CLI's own active session; that is separate from this data-source read.)

## Config fields

| Field | Type | Default | Purpose |
|---|---|---|---|
| `claude_code_enabled` | `bool` | true | Master toggle. |

## Rust adapter

```rust
pub struct ClaudeCodeDataSource;

#[async_trait]
impl DataSource for ClaudeCodeDataSource {
    fn id(&self) -> &str { "claude-code" }
    fn display_name(&self) -> &str { "Claude Code sessions" }
    fn is_available(&self) -> bool {
        dirs::home_dir().map(|h| h.join(".claude").exists()).unwrap_or(false)
    }
    async fn gather(&self, window: &DateWindow, config: &AppConfig)
        -> Result<SourceData, DataSourceError> {
        // 1. scan ~/.claude/projects/*/*.jsonl
        // 2. for each line: route by type, apply filters
        // 3. collect <=15 deduped prompt snippets
        // 4. scan ~/.claude/plans/*.md by mtime
        // 5. collect edited file basenames, attribute to repos
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

### CONTEXT block (enrichment)
```
## CONTEXT
plans:
- Plan title 1
- Plan title 2
prompts:
- snippet 1 (<=200 chars)
- snippet 2
- ...
```

### CLAUDE-FILES (files)
Per-repo list of edited file basenames:
```
### files-edited: main.rs, lib.rs, config.rs
```
Only repos with at least one edited file appear.

## Cache

- TTL: **2700 seconds** (45 minutes).
- Cache key includes the `DateWindow` hash and the source id `claude-code`.
- Cached payload: both `enrichment` (conv digest) and `files`.

## Meta-work filter (`standup_meta`)

A regex named `standup_meta` drops standup-tooling self-references so the conversation digest does not contain entries like "generate my standup" or "run scrub". It matches (case-insensitive) terms including but not limited to:

- `standup`, `daily-standup`
- `compile.sh`, `compile-standup`
- `scrub`, `scrub_notes`
- `render-prompt`, `render`
- `anti-backdate`, `backdate`
- any phrase in the `STANDUP_META_EXTRA` config list (user-extensible).

Lines whose user-prompt text matches `standup_meta` are excluded from the 15-snippet cap.