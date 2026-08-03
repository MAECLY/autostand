# Anti-Backdating Spec

This document specifies how `autostand` prevents the standup from claiming work was done on a day it wasn't. It covers the three attack vectors, definitions, the scrub algorithm, SKEW detection, accumulate-never-delete, freeze, self-heal, and test cases.

---

## Problem

A standup is a claim that work happened in a specific window. Without guards, three failure modes let a standup claim work it never did:

1. **Notes restating committed work** — a note like `"merged FIF-133 into main"` names a ticket whose commit is already in git FACTS for the window. If the note is rendered alongside the git facts, the standup double-claims the same work and looks busier than it was.
2. **Notes naming tickets whose commits are on a different day** — a note like `"pushed FIF-140 yesterday"` written today, but FIF-140's commits are on a day outside the window. Rendering the note would back-date the work into the window.
3. **Meta-work about the standup tooling itself** — a note or commit like `"wrote the standup render prompt"` is work *about* the standup, not work *reported by* the standup. Including it creates a recursive loop where the standup grows itself.

`autostand` treats these as correctness bugs, not style choices. Every render must pass the scrub, and every render produces an audit sidecar that lets `audit` classify each bullet as `commit`/`github`/`review`/`note`/`phantom`/`unverified`.

---

## Definitions

| Term | Definition |
| --- | --- |
| `range_tickets` | tickets appearing in GIT FACTS for the window |
| `note_tickets` | tickets appearing in NOTES text (parsed from bullet clauses) |
| `all_git_tickets` | full ticket→commit-day map from `git log --all` (every ticket ever touched, with the dates of its commits) |
| `FORBIDDEN` | `note_tickets` whose commits are on a day NOT in the window (cross-day backdating) |
| `COVERED` | `note_tickets` already in `range_tickets` or GITHUB (don't duplicate from notes) |
| `CLAIM regex` | `\b(commits?\|committed\|PR\|merged\|pushed)\b` — clauses asserting committed work |
| `SKEW` | a note dated in-range naming a ticket whose commits ALL fall outside the window (stronger than FORBIDDEN — the entire ticket is out of range) |
| `META` | regex matching standup-tooling self-references: `standup`, `daily-standup`, `compile.sh`, `scrub`, `auto-compile`, `render-prompt`, `anti-backdate`, plus user-supplied `STANDUP_META_EXTRA` |

### Ticket parsing

Tickets are Jira keys matching `/\b([A-Z][A-Z0-9]+-\d+)\b/`. The same regex is used for FACTS, NOTES, and `all_git_tickets`.

### CLAIM regex

```regex
\b(commits?|committed|PR|merged|pushed)\b
```

Case-insensitive. Matches clauses that assert a commit/merge/push happened — these are dropped from notes because the git FACTS are the authoritative record of commits. Notes are for *non-commit* work (design, review, debugging, meetings).

### META regex

```regex
(standup|daily-standup|compile\.sh|scrub|auto-compile|render-prompt|anti-backdate)
```

Extended at runtime by `STANDUP_META_EXTRA` (pipe-separated). E.g. `STANDUP_META_EXTRA="my-standup-tool|foo-scrubber"` adds those alternatives.

---

## Scrub algorithm

```
function scrub_notes(notes, forbidden, covered, meta_regex, claim_regex):
    for each note in notes:
        for each clause in note.clauses:
            if clause matches claim_regex:
                drop clause        # commits are authoritatively in git FACTS
                continue
            tickets_in_clause = parse_tickets(clause)
            if any(t in forbidden for t in tickets_in_clause):
                # cross-day backdating attempt
                if alias_scrub and has_alias(t, clause):
                    re-attach alias tag to surviving non-commit work (if any)
                else:
                    drop clause
                continue
            if any(t in covered for t in tickets_in_clause):
                drop clause        # already in git FACTS or GITHUB
                continue
            if clause matches meta_regex:
                drop clause        # standup tooling self-reference
                continue
            keep clause
    return surviving clauses
```

### What "drop clause" means

A clause is a single bullet line in a note. Dropping it removes the bullet entirely. The note file on disk is **never** modified — only the in-memory copy sent to the renderer. The audit sidecar records which clauses were dropped and why.

### Alias scrub (optional)

When `scrub.alias_scrub == true`, a FORBIDDEN ticket with a **feature alias** (≥`alias_scrub_min` token overlap with the clause) keeps its tag attached to surviving non-commit work instead of being dropped. This is for the case where a ticket key is reused across a feature and the note describes the feature work, not the commit.

---

## SKEW detector

SKEW is a **signal**, not a rule. A note dated in-range names a ticket whose commits ALL fall outside the window. This is stronger than FORBIDDEN (which fires per-clause per-day). SKEW fires per-ticket.

```
function detect_skew(notes, all_git_tickets, window):
    for each note in notes:
        for each ticket in parse_tickets(note.text):
            commit_days = all_git_tickets.get(ticket)
            if commit_days is None: continue
            if not any(window.contains(d) for d in commit_days):
                # all commits are outside the window
                emit SkewRecord { ticket, note_date: note.date, commit_days }
                log_warning(f"⚠ SKEW ticket={ticket} note_date={note.date} commit_days={commit_days}")
```

- The note is **not dropped** (it may describe legitimate non-commit work on that ticket).
- The SKEW record is written to the audit sidecar so `audit` can flag it.
- The warning is logged to the run log.

---

## Accumulate-never-delete

Even after scrubbing, `accumulate` re-injects any PREV bullet not covered by the new render. This ensures **no work is lost** between runs.

```rust
pub fn accumulate(new_body: &str, prev_auto: &str, facts: &[RepoFacts]) -> String {
    let new_bullets   = extract_bullets(new_body);
    let prev_bullets  = extract_bullets(prev_auto);
    let mut kept = vec![];
    for prev in &prev_bullets {
        let covered = textsim::covered(prev, &new_bullets.iter().collect::<Vec<_>>(), facts);
        if !covered {
            kept.push(prev.clone());   // re-inject uncovered PREV bullet
        }
    }
    if kept.is_empty() { new_body.to_string() }
    else {
        format!("{new_body}\n\n<!-- re-injected from prior run -->\n{}", kept.join("\n"))
    }
}
```

`textsim::covered` uses significant-word overlap (threshold: ≥2 shared non-stopword tokens, or a shared ticket key). The same `textsim` module is used by `accumulate` and `audit` so "covered" means the same thing in both places.

---

## Freeze previous day

Once `F_PREV`'s AUTO block is populated and the day has rolled over (`F_PREV != F_TODAY`), `autostand` does **not** re-touch it. This prevents churn: the previous day's standup is final.

```rust
pub fn is_frozen(f_prev: &NaiveDate, f_today: &NaiveDate, host: &str) -> bool {
    if f_prev == f_today { return false; }          // same day → never freeze
    let existing = parse_file(&dailies_dir().join(format!("{}.md", f_prev)));
    existing.and_then(|f| f.auto_for(host).map(|b| !b.body.is_empty()))
        .unwrap_or(false)
}
```

`self_heal` checks `is_frozen` first and skips if true.

---

## Self-heal

If `F_PREV`'s AUTO block is empty (missed run), `self_heal` compiles it from durable disk data. Durable = git log + notes files still on disk. This covers the case where the machine was asleep at the scheduled time.

```rust
pub async fn self_heal(f_prev, host, app) -> Result<CompileResult> {
    if is_frozen(f_prev, &f_today, host) {
        return Ok(CompileResult::skip(f_prev, host, "frozen"));
    }
    compile_file(f_prev, &ctx(app, TriggerSource::SelfHeal)).await
}
```

Self-heal runs after `compile_file(F_TODAY)` in `trigger`. It reuses the same cache (the window for `F_PREV` is different from `F_TODAY`'s window, so cache keys differ).

---

## Test cases

| Test | Input | Expected behavior |
| --- | --- | --- |
| **FIF-133 phantom** (note restating committed work) | Note: `"merged FIF-133 into main"`. FIF-133 commits in window FACTS. | `scrub_notes` drops the note clause (matches CLAIM regex). Standup shows FIF-133 from git FACTS only. |
| **Cross-day ticket** | Note: `"pushed FIF-140 yesterday"`. FIF-140 commits on day `D` outside window. | FORBIDDEN includes FIF-140. Clause dropped. Audit records `forbidden_tickets: ["FIF-140"]`. |
| **Covered ticket** | Note: `"reviewed FIF-150 PR"`. FIF-150 in GITHUB block (PR review). | COVERED includes FIF-150. Clause dropped (don't duplicate). |
| **Meta-work** | Note: `"wrote the standup render prompt"`. | Matches META regex. Clause dropped. |
| **SKEW** | Note (dated in window): `"worked on FIF-200 design"`. FIF-200 commits all on day before `range_start`. | SKEW record emitted. Note **kept** (non-commit design work). Warning logged. |
| **Alias scrub on** | `alias_scrub=true`. Note: `"FIF-140 feature spike"` where FIF-140 is FORBIDDEN. Clause has ≥2 token overlap with FIF-140. | Alias tag re-attached; clause kept (with tag). |
| **Alias scrub off** | Same input, `alias_scrub=false`. | Clause dropped (FORBIDDEN rule fires). |
| **Accumulate re-injection** | PREV has bullet `"Refactored queue"`. New render omits it. `textsim::covered` returns false. | `accumulate` re-injects the bullet under `<!-- re-injected from prior run -->`. `accumulated_count = 1`. |
| **Freeze** | `F_PREV != F_TODAY`. `F_PREV` AUTO block non-empty. | `self_heal` returns `Skip("frozen")`. No re-compile. |
| **Self-heal** | `F_PREV != F_TODAY`. `F_PREV` AUTO block empty. Git log + notes on disk. | `self_heal` compiles `F_PREV` from durable data. |