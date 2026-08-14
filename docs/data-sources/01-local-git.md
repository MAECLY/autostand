# Local Git (`local-git`)

Local Git is the **authoritative source** for committed work. It scans every repository under `GITHUB_DIR`, runs `git log` filtered to the standup authors and date window, and emits a structured FACTS block that the renderer treats as ground truth.

## What it provides

- Per-repo commits in the window (subject, branch, author).
- Ticket keys extracted from commit subjects (e.g. `FIF-133`).
- Branch the commit landed on.
- File-level scope: top files by churn (changes count + insertions/deletions) and derived areas.
- Full ticket → commit-day map (used by the anti-backdating guard).
- `SourceData { facts: Some(...), notes: None, enrichment: None, files: None }`.

Local Git never emits `notes`, `enrichment`, or `files` — the `files` field here is left `None`; file attribution for *uncommitted* edits is the job of the AI CLI sources.

## Paths read

- **`GITHUB_DIR`** (default `~/Documents/Github`, configurable via `github_dir`).
- The adapter scans `GITHUB_DIR` at **maxdepth 1**. Any immediate child directory that contains a `.git` directory is treated as a repository.
- Non-git subdirectories are silently skipped.
- The adapter never writes to any repository; all `git` invocations use `-C <repo>` and read-only subcommands (`log`, `show`, `for-each-ref`).

## Git commands

### Commits in the window
```
git -C <repo> log --all --no-merges \
  --since=<start> --until=<end> \
  --author=<authors_regex> \
  --format=%s
```
- `--all` (overridable via `git_refs`) scans all refs. `git_refs` is split on whitespace, so a multi-token value (`--branches --tags`) becomes several arguments rather than one bogus one.
- `--no-merges` drops merge commits.
- **One `--author=<value>` flag per entry** in `standup_authors`. git ORs repeated `--author` options, but a single value is a *basic* regular expression in which `|` is a literal character — a `|`-joined alternation matches nothing.
- `--format=%s` returns only the subject line.

### Ticket keys
Extracted from commit subjects via regex:
```
\b([A-Z]+-\d+)\b
```
Keys are deduped per repo and listed in the FACTS header.

### File scope (top files by churn + areas)
```
git -C <repo> log --all --no-merges \
  --since=<start> --until=<end> \
  --author=<authors_regex> \
  --numstat --format=
```
- `--numstat` yields `added\tdeleted\tpath` per file per commit.
- The adapter aggregates per path: total changes, total `+`, total `-`.
- Top files by churn are listed in the FACTS block.
- **Areas** are derived from the top-level directory of each changed path (e.g. `src/main.rs` → `core`, `src/adapters/foo.rs` → `adapters`). Areas are deduped and comma-joined.

### Full ticket → commit-day map (anti-backdating)
```
git -C <repo> log --all --format=%cd|%s --date=short
```
- Not windowed — captures the entire history of every ref.
- Builds a `HashMap<TicketKey, Vec<NaiveDate>>` for the anti-backdating guard, which flags tickets claimed "today" that actually only have commits on a prior date.

## Auth

None. Local git reads are unauthenticated. The only requirement is that the commit author matches one of the entries in `standup_authors`; otherwise `--author` will not match and the repo would appear empty — see *Failure modes* below for why that can no longer pass unnoticed.

## Author resolution

1. `standup_authors`, trimmed and deduped. Blank entries do not count.
2. If that list is empty: this machine's `git config --get user.email`, or `user.name` when no email is configured.
3. If neither resolves: `DataSourceError::Misconfigured` — the gather fails loudly instead of returning an empty FACTS block.

Step 2 exists because `standup_authors` has no Settings control, so an install that never hand-edits `config.json` carries `[]`. Falling back to the machine identity is deliberately narrow — an *unfiltered* `git log` would report the whole team's commits as the user's own work, which is worse than reporting none. Email alone (not email *and* name) for the same reason: a display name is a basic regex that a teammate's name can also satisfy.

## Failure modes

Local Git is the authoritative source: `provenance::compute` derives `range_tickets` / `covered_tickets` from its FACTS block, and `render::validate_render`'s no-work-claim and out-of-range-ticket checks key on those. An empty FACTS block therefore does not merely lose data — it disarms the validation, and nothing downstream can tell it apart from a day off.

So every "gathered nothing" path is a `DataSourceError::Misconfigured`, surfaced as a `gather_failure` in the audit sidecar and in the UI:

| Situation | Outcome |
|---|---|
| `github_dir` is not a directory | error |
| `github_dir` holds no depth-1 git repos | error |
| No author filter resolvable (rule 3 above) | error |
| Filter resolved, but zero commits matched **and** other identities did commit in the window | error, quoting the identities that did commit |
| Zero commits matched and `git log` failed in some repos | error, quoting the git failures |
| Zero commits matched and nobody at all committed in the window | `Ok(SourceData::default())` — the one honest empty result |
| Some repos matched, others' `git log` failed | partial FACTS + a `tracing::warn!` per failed repo |

The identity probe (`git log … --format=%ae|%ce`, no `--author`) only runs when the filtered scan came back empty, so it costs nothing on the path where the filter works.

## Config fields

| Field | Type | Default | Purpose |
|---|---|---|---|
| `github_dir` | `PathBuf` | `~/Documents/Github` | Root directory scanned for repos. |
| `standup_authors` | `Vec<String>` | `[]` → machine git identity | Author emails/names matched by `--author`. |
| `git_refs` | `String` | `--all` | Ref selector passed to `git log`; split on whitespace. |

## Rust adapter

```rust
pub struct LocalGitDataSource {
    github_dir: PathBuf,
    authors: Vec<String>,
    git_refs: String,
}

#[async_trait]
impl DataSource for LocalGitDataSource {
    fn id(&self) -> &str { "local-git" }
    fn display_name(&self) -> &str { "Local Git" }
    fn is_available(&self) -> bool { self.github_dir.is_dir() }
    async fn gather(&self, window: &DateWindow, config: &AppConfig)
        -> Result<SourceData, DataSourceError> {
        // 1. scan github_dir at maxdepth 1 for .git children
        // 2. for each repo: run the git log commands above
        // 3. build per-repo FACTS block
        // 4. build ticket->commit-day map for anti-backdating
        Ok(SourceData {
            facts: Some(facts_block),
            notes: None,
            enrichment: None,
            files: None,
        })
    }
}
```

## Output format (FACTS block)

The `facts` string is a concatenation of per-repo sections:

```
### repo: <name> / tickets: FIF-133 FIF-140 / commits (3):
- [FIF-133] commit subject 1 (branch: main)
- [FIF-133] commit subject 2 (branch: main)
- [FIF-140] commit subject 3 (branch: feature/x)
files: 5 changed, +120/-30
  - src/main.rs (8 changes, +45/-12)
  - src/lib.rs (5 changes, +30/-8)
areas: core, adapters
```

- Repos with zero matching commits produce no section (omitted, not an empty header).
- `commits (N)` is the count of matched non-merge commits.
- `files:` lists top files by churn (max ~8); the `N changed, +X/-Y` line is the aggregate.
- `areas:` lists deduped top-level dirs of changed files.

## Anti-regression guard

If the FACTS block is empty for this run, but a prior run recorded repos for the same state file (`last-<F>-<HOST>.facts`), the adapter **skips the empty result** rather than emitting an empty FACTS block. This handles transient empty reads — e.g. when Syncthing is mid-sync and `.git` directories are momentarily incomplete.

The guard compares:
- current repo set (scanned now) vs.
- last successful repo set (stored in state).

If the current set is empty/strictly-smaller **and** the last set was non-empty, the prior FACTS block is reused for this run and a warning is logged.

## Always enabled

Local Git is the authoritative source and **cannot be disabled**. There is no `local_git_enabled` flag. Even if a user attempts to toggle it off in config, the pipeline forces it on.