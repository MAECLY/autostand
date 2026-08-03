# Remember plugin (`remember-plugin`)

The Remember plugin source consumes output files written by the external Claude Code plugin `claude-plugins-official/remember`. The plugin itself is **not** part of autostand — autostand only reads the `.remember` files it produces.

## What it is

`claude-plugins-official/remember` is a Claude Code plugin that captures session notes into per-repo `.remember` folders and a central rolling note. autostand treats these files as a **narrative, last-resort** input: after scrubbing, notes are demoted below git, github, and file-attribution sources.

## What it provides

- Narrative non-commit work notes — free-text clauses the user typed during the day.
- `SourceData { facts: None, notes: Some(notes_text), enrichment: None, files: None }`.

Notes are the lowest-priority tier. The renderer only falls back to them when git/github/files do not fully explain the day.

## Paths read (read-only)

- **Per-repo `.remember` folders**, discovered by scanning `GITHUB_DIR` at **maxdepth 3** for any directory named `.remember`. Inside each, the adapter reads:
  - `today-YYYY-MM-DD.md` — today's note file for that repo.
  - `today-YYYY-MM-DD.done.md` — completed/archived note for that repo.
- **Central rolling note**: `$GITHUB_DIR/.remember/now.md` — read **only when `range_end == today`**. This file rolls forward through the day and is not meaningful for historical windows.

The adapter never writes to `.remember` — the plugin owns writes.

## File format

Each `.remember` note file uses this structure:

```
## HH:MM | <session>
free-text clause one.
free-text clause two.
## HH:MM | <next session>
...
```

The adapter parses the `## HH:MM | <session>` header lines to delimit clauses but preserves the free-text body verbatim for the renderer.

## Auth

None. The files are local markdown on disk.

## Config fields

| Field | Type | Default | Purpose |
|---|---|---|---|
| `remember_enabled` | `bool` | true | Master toggle. |

## Rust adapter

```rust
pub struct RememberDataSource {
    github_dir: PathBuf,
}

#[async_trait]
impl DataSource for RememberDataSource {
    fn id(&self) -> &str { "remember-plugin" }
    fn display_name(&self) -> &str { "Remember plugin" }
    fn is_available(&self) -> bool { self.github_dir.is_dir() }
    async fn gather(&self, window: &DateWindow, config: &AppConfig)
        -> Result<SourceData, DataSourceError> {
        // 1. scan GITHUB_DIR maxdepth 3 for .remember dirs
        // 2. read today-<date>.md and today-<date>.done.md per repo
        // 3. if range_end == today, read $GITHUB_DIR/.remember/now.md
        // 4. concatenate clauses into notes_text
        Ok(SourceData {
            facts: None,
            notes: Some(notes_text),
            enrichment: None,
            files: None,
        })
    }
}
```

## Scrubbing

After `gather()`, notes are passed through `scrub_notes()`, which runs **after** the source returns and before the renderer sees the notes:

- **CLAIM regex clauses** — dropped. Anything matching the CLAIM pattern (e.g. assertions of work done without evidence) is removed.
- **FORBIDDEN/COVERED ticket clauses** — dropped. If a clause references a ticket already covered by git/github/files, it is removed to avoid duplication.
- **Meta-work** — `standup_meta` regex (same one used by Claude Code) drops standup-tooling self-references.
- **SKEW detector** — runs on the surviving notes to flag time-skew anomalies (e.g. a clause timestamped outside the window).

## TCC (macOS Transparency, Consent, and Control)

On macOS, reading files under a Syncthing-managed or TCC-protected directory may be denied by the OS consent system. The adapter handles this **gracefully**:

- It branches on **read error** (permission denied / TCC denial), not on empty content. An empty file is a valid note; a denied file is a transport error.
- On denial, it logs a warning and skips that specific file. It does **not** crash the standup run.
- The user is expected to grant Terminal/iTerm (or the Tauri app bundle) Full Disk Access in System Settings → Privacy & Security if they want `.remember` reads to succeed under TCC.

## Output format

The raw surviving note clauses (post-scrub) are passed to the renderer as the **NOTES** input. The renderer decides how to weave them into the final standup given the priority hierarchy (git > github > files > notes).