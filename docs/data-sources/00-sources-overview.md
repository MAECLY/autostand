# Data Sources Overview

`autostand` aggregates daily-standup facts from **8 read-only data sources**. Each source implements the `DataSource` trait and feeds the pipeline either authoritative committed work (git/github), file attribution + conversation digests (AI CLI sessions), or narrative notes (Remember plugin).

## The 8 sources

| # | Source id | Display name | Provides | Storage type | Default enabled |
|---|---|---|---|---|---|
| 1 | `local-git` | Local Git | Commits, ticket keys, branch, file scope, anti-backdating map | Local `.git` under `GITHUB_DIR` | **Always** (cannot be disabled) |
| 2 | `github` | GitHub (via `gh` CLI) | PRs opened/merged, review/issue comments, review states, recent-PR tickets | Remote via `gh` CLI (OAuth) | Yes |
| 3 | `claude-code` | Claude Code sessions | Conversation digest + edited file attribution | `~/.claude/projects/*/*.jsonl`, `~/.claude/plans/*.md` | Yes |
| 4 | `remember-plugin` | Remember plugin | Narrative non-commit work notes | `<repo>/.remember/*.md`, `$GITHUB_DIR/.remember/now.md` | Yes |
| 5 | `opencode` | OpenCode | Session history + edited file attribution | `~/.local/share/opencode/opencode.db` (SQLite), legacy JSON | Yes |
| 6 | `codex` | Codex CLI | Session history + edited file attribution | `~/.codex/sessions/YYYY/MM/DD/*.jsonl`, `~/.codex/history.jsonl` | No (opt-in) |
| 7 | `gemini-cli` | Gemini CLI | Session history + edited file attribution | `~/.gemini/**/*.jsonl` | No (opt-in) |
| 8 | `grok-cli` | Grok CLI | Session history + edited file attribution | `~/.grok/` or `~/.config/grok-cli/` (variant-dependent) | No (opt-in) |

## The `DataSource` trait

Every source implements this trait in `src-tauri/src/sources/mod.rs`:

```rust
#[async_trait]
pub trait DataSource: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn is_available(&self) -> bool;
    async fn gather(&self, window: &DateWindow, config: &AppConfig) -> Result<SourceData, DataSourceError>;
}

pub struct DateWindow {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub dates: Vec<NaiveDate>,
}

pub struct SourceData {
    pub facts: Option<String>,       // structured facts (git-style)
    pub notes: Option<String>,       // narrative notes
    pub enrichment: Option<String>,  // conversation/file context
    pub files: Vec<String>,          // edited file basenames per repo
}
```

- `id()` — stable identifier used in config keys, cache namespaces, and state files.
- `display_name()` — human label for logs and the Settings UI.
- `is_available()` — quick check (e.g. is `~/.claude` present? is `gh` on PATH and authenticated?).
- `gather()` — the heavy work. Returns a `SourceData` whose populated fields depend on the source (see per-source docs).

## Priority hierarchy

When multiple sources contribute evidence for the same ticket or file, the renderer applies this priority. Higher tiers overwrite/demote lower tiers during the scrub phase.

| Tier | Sources | Role |
|---|---|---|
| **GIT** | `local-git` | Authoritative commits. Commit subjects, ticket keys, branch, churn — ground truth. |
| **GITHUB** | `github` | Authoritative PRs, reviews, issue comments. Used to corroborate git activity and add review/comment context. |
| **FILES** | `claude-code`, `opencode`, `codex`, `gemini-cli`, `grok-cli` | Non-commit file attribution. If a file was edited in an AI CLI session but NOT committed, these sources surface it. They never override git; they only fill gaps. |
| **NOTES** | `remember-plugin` | Narrative, last resort. Demoted after scrubbing — used only when git/github/files do not explain the day. |

## Read-only contract

**All sources are read-only.** The app never:

- writes to source repositories,
- mutates AI session directories (`~/.claude`, `~/.codex`, `~/.gemini`, `~/.grok`, `~/.local/share/opencode`),
- pushes to GitHub,
- modifies `gh` auth state.

The only writes autostand performs are to its own state directory (cache, last-run files, compiled standup output).

## Toggle / enablement

Each source has an `enabled` bool in `AppConfig`:

| Source | Config key | Default | Notes |
|---|---|---|---|
| `local-git` | — | **forced on** | Authoritative; cannot be disabled. |
| `github` | `github_enabled` | true | Requires `gh auth login`. |
| `claude-code` | `claude_code_enabled` | true | |
| `remember-plugin` | `remember_enabled` | true | |
| `opencode` | `opencode_enabled` | true | |
| `codex` | `codex_enabled` | false | Opt-in (Codex is newer). |
| `gemini-cli` | `gemini_cli_enabled` | false | Opt-in. |
| `grok-cli` | `grok_cli_enabled` | false | Opt-in. |

Disabled sources are skipped entirely — `is_available()` is not even consulted.

## Cache

Enrichment sources (everything **except** `local-git` and `remember-plugin`) are cached to avoid redundant network/IO on hourly runs:

- Cache backend: on-disk JSON under the autostand state dir, namespaced per source id + `DateWindow` hash.
- **TTL: 2700 seconds (45 minutes).** This covers the hourly standup refresh cycle with margin.
- `local-git` is never cached — it must reflect the latest `git log`.
- `remember-plugin` notes are not cached (user may edit `.remember` files between runs).

Cache hits return the prior `SourceData` without invoking `gather()` again. Cache misses/refreshes call `gather()` and store the result with a timestamp.