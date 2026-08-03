# OpenCode (`opencode`)

The OpenCode source reads the OpenCode CLI's session storage. It produces a conversation digest (user-typed prompts) and an edited-file attribution list (file paths from tool calls, attributed to repos under `GITHUB_DIR`).

## What it provides

1. **Session history** — user-typed prompts from OpenCode sessions whose `created_at` falls inside the `DateWindow`.
2. **Edited file attribution** — file paths from tool-call parts, attributed to repos by path-prefix match against `GITHUB_DIR` children.

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

- **Primary**: `~/.local/share/opencode/opencode.db` — SQLite database. The adapter queries this first.
- **Legacy fallback**: `~/.local/share/opencode/storage/session/*.json` — JSON session files (older OpenCode format). The adapter falls back to these if the SQLite DB is absent.
- **Config presence check**: `~/.local/share/opencode/config.json` **or** `~/.config/opencode/config.json` — **not read for data**. The adapter only checks for the existence of one of these to confirm OpenCode is installed before attempting a data read.

The adapter never writes to the SQLite DB or the session JSON files. SQLite is opened read-only.

## SQLite schema (discovered)

| Table | Key columns |
|---|---|
| `session` | `id`, `created_at`, ... |
| `message` | `id`, `session_id`, `role`, `created_at`, ... |
| `part` | `id`, `message_id`, `type`, `text`, ... |

### User-prompt query
```sql
SELECT text
FROM part
JOIN message ON part.message_id = message.id
JOIN session ON message.session_id = session.id
WHERE message.role = 'user'
  AND session.created_at BETWEEN ? AND ?;
```
- The two bind parameters are the window start/end (as Unix timestamps or ISO strings per the column type).
- Returned `text` rows are filtered with the same rules as Claude Code (slash commands, XML wrappers, "you are" meta, code pastes, `standup_meta`).
- Max 15 deduped snippets, ≤ 200 chars each.

### Edited files

- `part` rows whose `type` indicates a file-operation tool call.
- Extract path keys from the part payload (`file_path`, `path` — field names probed defensively).
- Attribute to repos by path-prefix match against `GITHUB_DIR`.
- Only edit-class tools counted (Edit/Write/Update/Create equivalents).

## Auth

None. The SQLite DB and JSON files are local and unauthenticated.

## Config fields

| Field | Type | Default | Purpose |
|---|---|---|---|
| `opencode_enabled` | `bool` | true | Master toggle. |

## Rust adapter

```rust
pub struct OpenCodeDataSource;

#[async_trait]
impl DataSource for OpenCodeDataSource {
    fn id(&self) -> &str { "opencode" }
    fn display_name(&self) -> &str { "OpenCode" }
    fn is_available(&self) -> bool {
        // ~/.local/share/opencode/opencode.db exists
        // OR ~/.local/share/opencode/storage/session/ has *.json
        // AND a config.json is present somewhere expected
    }
    async fn gather(&self, window: &DateWindow, config: &AppConfig)
        -> Result<SourceData, DataSourceError> {
        // 1. try SQLite: open read-only, run the user-prompt query
        // 2. if DB missing, fall back to legacy JSON session files
        // 3. collect <=15 deduped snippets + edited file paths
        Ok(SourceData {
            facts: None,
            notes: None,
            enrichment: Some(conv_digest),
            files: Some(edited_files),
        })
    }
}
```

SQLite access uses `rusqlite` with the **`bundled`** feature so the user does not need a system SQLite library. The connection is opened with `OpenFlags::SQLITE_OPEN_READ_ONLY`.

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
- Cache key includes the `DateWindow` hash and source id `opencode`.
- Cached payload: both `enrichment` and `files`.