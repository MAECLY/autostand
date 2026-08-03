# GitHub (`github`, via `gh` CLI)

The GitHub source queries GitHub through the official `gh` CLI. It contributes PRs opened/merged, your review and issue comment bodies, the review state you applied to PRs you reviewed, and a recent-PR tickets trailer. It emits only `enrichment`; it never produces facts, notes, or file lists.

## What it provides

- **PRs opened**: PRs where `--author=@me` and `createdAt` falls inside the `DateWindow`.
- **PRs merged**: PRs where `--author=@me` and `closedAt` falls inside the `DateWindow` (state `MERGED`).
- **Your review/comment bodies**: review comments and issue comments authored by `reviewer` with `created_at` inside the window, truncated to `comment_len` characters (default 220).
- **PRs you reviewed**: PRs found via `--reviewed-by=@me`, enriched with your review state (`APPROVED` / `CHANGES_REQUESTED` / `DISMISSED` / `COMMENTED`).
- **Recent-PR tickets trailer**: `<!-- GH-RECENT-TICKETS: FIF-133 FIF-140 -->` — deduped ticket keys extracted from recent PR titles, emitted as an HTML comment for downstream tooling.
- `SourceData { facts: None, notes: None, enrichment: Some(github_block), files: None }`.

## Auth

`gh` CLI OAuth session, established out-of-band by the user via `gh auth login`. No API token is stored in `autostand` config.

- Session file: `~/.config/gh/hosts.yml` (macOS/Linux), `%APPDATA%\GitHub CLI\hosts.yml` (Windows).
- The adapter shells out to `gh` and inherits the session; it never reads the token directly.
- A "Test gh auth" button in Settings runs `gh auth status` to surface auth failures to the user.

## `gh` CLI commands

### PRs opened/merged
```
gh search prs --author=@me --owner=<org> \
  --json number,title,createdAt,closedAt,state,repository
```
- `<org>` is `pr_org` (default `fifty-fit`, configurable).
- The adapter filters the JSON result by `createdAt`/`closedAt` against the `DateWindow` (the GitHub search API's date semantics are coarse; precise filtering is client-side).
- Capped at `max_prs` (default 10).

### PRs reviewed
```
gh search prs --reviewed-by=@me \
  --json number,title,repository
```
- Returns PRs you reviewed (any state). The adapter then fetches your review state per PR.

### Review state
```
gh pr view <num> -R <repo> --json reviews
```
- Filters the `reviews` array by `reviewer.login == reviewer` and `submittedAt` inside the window.
- Picks the latest review state for the window: `APPROVED`, `CHANGES_REQUESTED`, `DISMISSED`, or `COMMENTED`.
- `include_self_reviews` (default false) controls whether PRs you both authored and reviewed are counted.

### Review & issue comments
```
gh api /repos/<repo>/pulls/<num>/comments
gh api /repos/<repo>/issues/<num>/comments
```
- Filter by `user.login == reviewer` and `created_at` inside the window.
- Truncate body to `comment_len` (220) characters.
- Dedupe across the two endpoints by `(url, created_at)`.

## Config fields

| Field | Type | Default | Purpose |
|---|---|---|---|
| `github_enabled` | `bool` | true | Master toggle for the source. |
| `reviewer` | `String` | (from config) | GitHub login used to filter reviews/comments. |
| `pr_org` | `String` | `fifty-fit` | Org passed to `--owner`. |
| `max_prs` | `u32` | 10 | Max PRs returned per query. |
| `comment_len` | `usize` | 220 | Truncation length for comment bodies. |
| `include_self_reviews` | `bool` | false | Include PRs you both authored and reviewed. |

## Rust adapter

```rust
pub struct GithubDataSource {
    reviewer: String,
    pr_org: String,
    max_prs: u32,
    comment_len: usize,
    include_self_reviews: bool,
}

#[async_trait]
impl DataSource for GithubDataSource {
    fn id(&self) -> &str { "github" }
    fn display_name(&self) -> &str { "GitHub (via gh CLI)" }
    fn is_available(&self) -> bool {
        // `gh` on PATH AND `gh auth status` succeeds
    }
    async fn gather(&self, window: &DateWindow, config: &AppConfig)
        -> Result<SourceData, DataSourceError> {
        // 1. gh search prs --author=@me
        // 2. gh search prs --reviewed-by=@me
        // 3. gh pr view for review states
        // 4. gh api for review/issue comments
        // 5. assemble github_block
        Ok(SourceData {
            facts: None,
            notes: None,
            enrichment: Some(github_block),
            files: None,
        })
    }
}
```

## Output format

```
## GITHUB ACTIVITY
PRs opened:
- #42 Title (repo: fifty-fit/api, createdAt: 2026-08-03)
PRs merged:
- #40 Title (repo: fifty-fit/web, mergedAt: 2026-08-03)
Reviews:
- #38 Title (repo: fifty-fit/api, state: APPROVED)
Comments:
- #38 "first 220 chars of comment body..."
<!-- GH-RECENT-TICKETS: FIF-133 FIF-140 -->
```

The `GH-RECENT-TICKETS` trailer is an HTML comment so it is invisible in rendered markdown but parseable by downstream tooling (e.g. the anti-backdating guard and ticket-link enricher).

## Timeout

Every `gh` CLI invocation is wrapped in a **60-second timeout**. If `gh` hangs (network issues, stale auth, rate limits), the adapter aborts the call, logs a warning, and returns whatever partial data it has rather than blocking the standup run.

## Cache

- TTL: **2700 seconds** (45 minutes).
- Cache key includes `reviewer`, `pr_org`, and the `DateWindow` hash.
- Cache is stored in the autostand state dir under `cache/github-<hash>.json`.

## Settings UI

The Settings panel exposes:
- Enable/disable toggle (`github_enabled`).
- Reviewer login text field.
- Org text field (`pr_org`).
- Max PRs number field.
- Comment length number field.
- Include self-reviews checkbox.
- **"Test gh auth" button** — runs `gh auth status` and shows success/failure inline.