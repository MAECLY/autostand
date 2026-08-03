# Audit Spec

This document specifies the audit sidecar: purpose, path, the `AuditData` schema, phantom detection, the classification algorithm, the UI, and the missing-sidecar fallback.

---

## Purpose

Every render produces a **provenance sidecar** — a JSON file recording exactly what inputs were used, what was scrubbed, what render mode ran, and whether it fell back. The sidecar enables:

1. **Read-only phantom detection** — after a standup is written, `audit` classifies each AUTO bullet as `commit`/`github`/`review`/`note`/`phantom`/`unverified` by matching it against the sidecar. Phantoms (code-change bullets on FORBIDDEN tickets with no matching fact/note) fail the audit.
2. **Debugging** — the Debug page (`routes/debug.tsx`) shows the sidecar JSON alongside the gathered inputs so you can trace why a bullet was (or wasn't) included.
3. **Reproducibility** — the sidecar's `hash` field is the inputs hash; if it matches the persisted hash, `dirty_check` skips a re-render.

The sidecar is **never** committed to git. It lives in `state/audit/` with permissions `0600`.

---

## Sidecar path

```
<state_dir>/audit/<F>-<HOST>.json
```

- `<state_dir>` = platform state dir (see `docs/tauri/03-platform-targets.md`).
- `<F>` = filing date, e.g. `2026-08-03`.
- `<HOST>` = host slug, e.g. `MacStudio-de-Miguel`.
- Example: `state/audit/2026-08-03-MacStudio-de-Miguel.json`.
- Permissions: `0600` (owner read/write only). Contains redacted conversation digests.
- Write: atomic (temp → fsync → rename).

```rust
pub fn sidecar_path(state: &Path, f: &NaiveDate, host: &str) -> PathBuf {
    state.join("audit").join(format!("{}-{}.json", f, host))
}

pub fn write_audit(state: &Path, f: &NaiveDate, host: &str, data: &AuditData) -> Result<()> {
    let path = sidecar_path(state, f, host);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let bytes = serde_json::to_vec_pretty(data)?;
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    tmp.write_all(&bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(&path)?;
    set_mode_0600(&path)?;
    Ok(())
}
```

---

## `AuditData` schema

### Rust

```rust
// crates/autostand-core/src/audit.rs
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditData {
    pub file: String,                      // "2026-08-03"
    pub host: String,                      // host slug
    pub rendered_at: DateTime<Utc>,
    pub window: DateRange,                 // range_start, range_end
    pub facts: Vec<RepoFacts>,             // per-repo commits, tickets, files
    pub notes: Vec<NoteRef>,                // source file paths + clauses (redacted)
    pub github: Option<String>,            // raw github block (redacted)
    pub conv: Option<String>,               // claude conversation digest (redacted)
    pub prrev: Option<String>,             // PR review section
    pub claude_files: Vec<String>,
    pub opencode_sessions: Vec<String>,
    pub codex_sessions: Vec<String>,
    pub gemini_sessions: Vec<String>,
    pub grok_sessions: Vec<String>,
    pub forbidden_tickets: Vec<String>,
    pub covered_tickets: Vec<String>,
    pub skew: Vec<SkewRecord>,              // { ticket, note_date, commit_days }
    pub ticket_days: HashMap<String, Vec<NaiveDate>>,
    pub render_mode: String,                // "auto" | "llm" | "det"
    pub render_used: String,                // "llm" | "det" | "llm_fallback"
    pub provider: Option<String>,           // which LLM provider was used
    pub model: Option<String>,
    pub fellback: bool,
    pub hash: String,                       // inputs hash
    pub accumulated_count: u32,             // bullets re-injected from PREV
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub range_start: NaiveDate,
    pub range_end: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoFacts {
    pub repo: String,
    pub ticket: Option<String>,
    pub title: String,
    pub commits: Vec<CommitInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub subject: String,
    pub date: NaiveDate,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRef {
    pub source: String,
    pub date: NaiveDate,
    pub clauses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkewRecord {
    pub ticket: String,
    pub note_date: NaiveDate,
    pub commit_days: Vec<NaiveDate>,
}
```

### TypeScript mirror (`src/lib/types.ts`)

```ts
export interface AuditData {
  file: string;
  host: string;
  rendered_at: string;            // ISO-8601 UTC
  window: { range_start: string; range_end: string };
  facts: RepoFacts[];
  notes: NoteRef[];
  github: string | null;
  conv: string | null;
  prrev: string | null;
  claude_files: string[];
  opencode_sessions: string[];
  codex_sessions: string[];
  gemini_sessions: string[];
  grok_sessions: string[];
  forbidden_tickets: string[];
  covered_tickets: string[];
  skew: SkewRecord[];
  ticket_days: Record<string, string[]>;
  render_mode: "auto" | "llm" | "det";
  render_used: "llm" | "det" | "llm_fallback";
  provider: string | null;
  model: string | null;
  fellback: boolean;
  hash: string;
  accumulated_count: number;
}
```

---

## Phantom detection

The `audit` command (exposed as `read_audit_sidecar` + a `classify` step on the frontend) classifies each AUTO bullet in the file against the sidecar.

### Classifications

| Badge | Class | Match rule |
| --- | --- | --- |
| 🟢 green | `commit` | matches a git fact (subject or ticket in `facts`) |
| 🔵 blue | `github` | matches a PR/review in the `github` or `prrev` block |
| 🟡 yellow | `note` | matches a surviving note clause (after scrub) |
| 🔴 red | `phantom` | a code-change bullet on a FORBIDDEN ticket with no matching fact/note |
| ⚪ gray | `unverified` | no matching source at all |

A `phantom` causes the audit to **fail** (exit 1 in CLI; red badge in UI). `unverified` is a warning (yellow badge) — it may be a legitimate note that the scrub kept but the classifier couldn't match.

### Classification algorithm

```rust
pub enum BulletClass { Commit, Github, Review, Note, Phantom, Unverified }

pub fn classify_bullet(bullet: &str, audit: &AuditData) -> BulletClass {
    let tickets = parse_tickets(bullet);

    // 1. commit: matches a git fact
    for fact in &audit.facts {
        if fact.ticket.as_deref().map(|t| tickets.contains(&t.to_string())).unwrap_or(false)
            || textsim::best_match(bullet, &fact.commits.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>()).is_some()
        {
            return BulletClass::Commit;
        }
    }
    // 2. github: matches PR/review
    if let Some(gh) = &audit.github {
        if textsim::best_match(bullet, &[gh.as_str()]).is_some() {
            return BulletClass::Github;
        }
    }
    // 3. review: matches PR review entry
    if let Some(rev) = &audit.prrev {
        if textsim::best_match(bullet, &[rev.as_str()]).is_some() {
            return BulletClass::Review;
        }
    }
    // 4. note: matches a surviving note clause
    for note in &audit.notes {
        if note.clauses.iter().any(|c| textsim::best_match(bullet, &[c.as_str()]).is_some()) {
            return BulletClass::Note;
        }
    }
    // 5. phantom: code-change bullet on a FORBIDDEN ticket with no match
    if tickets.iter().any(|t| audit.forbidden_tickets.contains(t))
        && looks_like_code_change(bullet)
    {
        return BulletClass::Phantom;
    }
    // 6. unverified
    BulletClass::Unverified
}
```

`looks_like_code_change(bullet)` checks for verbs implying a code change: `fixed|refactored|added|removed|implemented|wrote|migrated|deleted`. Bullets that are clearly non-code (`attended meeting`, `drafted doc`) don't trigger phantom even on a FORBIDDEN ticket — they'd classify as `unverified` instead.

---

## Shared `textsim`

The same `textsim` module is used by `accumulate` and `audit` so "covered" is consistent. A bullet is **covered** if:

- It shares a ticket key with a fact/note, OR
- It shares ≥2 non-stopword tokens (significant words) with a fact/note subject or clause.

```rust
// crates/autostand-core/src/textsim.rs
pub fn best_match<'a>(needle: &str, haystack: &'a [&'a str]) -> Option<&'a str> {
    let needle_tokens = significant_tokens(needle);
    haystack.iter()
        .map(|h| (h, significant_tokens(h)))
        .filter(|(_, hay)| {
            let shared = needle_tokens.iter().filter(|t| hay.contains(t)).count();
            shared >= 2
        })
        .map(|(h, _)| h)
        .next()
}

pub fn covered(prev: &str, others: &[&String], _facts: &[RepoFacts]) -> bool {
    let prev_tokens = significant_tokens(prev);
    others.iter().any(|o| {
        let o_tokens = significant_tokens(o);
        prev_tokens.iter().filter(|t| o_tokens.contains(t)).count() >= 2
    })
}

fn significant_tokens(s: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &["the","a","an","and","or","to","of","in","for","on","with","by","from","is","was","were","be","been","it","this","that","into","my","our"];
    s.split_whitespace()
        .filter(|w| !STOPWORDS.contains(&w.to_lowercase().as_str()))
        .map(|w| w.to_lowercase())
        .collect()
}
```

---

## UI

The Audit page (`routes/audit.tsx`) shows:

1. A date picker → `list_audit_sidecars(date)` returns one sidecar per host.
2. A per-host panel with the rendered standup bullets, each with a classification badge:
   - 🟢 `commit` (green) — bullet traces to a git fact
   - 🔵 `github` (blue) — bullet traces to a PR
   - 🟣 `review` (purple, mapped to `review`) — bullet traces to a PR review
   - 🟡 `note` (yellow) — bullet traces to a surviving note clause
   - 🔴 `phantom` (red) — code-change bullet on a FORBIDDEN ticket, no match → audit **fails**
   - ⚪ `unverified` (gray) — no match (warning, not failure)
3. A sidecar JSON viewer with expand/collapse (uses `ScrollArea` + `react-markdown` for pretty-printed JSON).
4. A SKEW table showing `ticket`, `note_date`, `commit_days` for each SKEW record.

### Failure semantics

- CLI (`autostand --audit <date>`): any `phantom` bullet → exit 1.
- UI: a red badge on the phantom bullet + a banner "Audit failed: N phantom bullets."

---

## If sidecar missing

If no sidecar exists for a date/host (e.g. the file was written by the App Script before `autostand` existed, or the state dir was wiped), `audit` reconstructs from durable disk data on a **best-effort** basis:

1. Re-run `gather_git_facts` for the window (from git log — always available).
2. Re-run `gather_notes` for the window (from note files on disk — usually available).
3. Reconstruct a minimal `AuditData` with `render_mode: "det"`, `render_used: "det"`, `fellback: true`, `provider: None`.
4. Log a warning `⚠ audit sidecar missing for <F>/<host>; reconstructed from disk (no enrichment)`.
5. Classify bullets against the reconstructed data. Enrichment-only bullets (PR reviews, conversations) will classify as `unverified` — this is expected and is the cost of the missing sidecar.

The reconstruction never **writes** a sidecar (it's a read-only audit pass). If you want to persist a reconstructed sidecar, run `autostand --audit <date> --persist` (CLI flag).