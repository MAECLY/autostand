# Pipeline Spec

This document specifies the `autostand-core::pipeline` module: the ordered steps of a compile, the cache, concurrency, error recovery, and a Mermaid flowchart with function names annotated.

---

## Pipeline steps

Each step is a function in `autostand-core::pipeline`. The entry point is `trigger(source)`. A single compile of one filing date is `compile_file(F, ctx)`. `trigger` may invoke `compile_file` for both `F_TODAY` and `F_PREV`.

### 1. `trigger(source: TriggerSource)`

Entry point. Acquires the single-run lock. Pulls the dailies repo. Computes target dates. Calls `compile_file` for each. Commits and pushes.

```rust
pub async fn trigger(source: TriggerSource, app: &AppHandle) -> Result<()> {
    let _lock = acquire_lock(state_dir(), LOCK_TIMEOUT).await?;
    emit(app, "pipeline-started", StartedPayload { trigger: source.clone(), .. })?;

    git_sync_pull(dailies_dir()).await?;            // git pull --rebase --autostash

    let today = chrono::Local::now().date_naive();
    let (f_today, f_prev) = compute_targets(today);

    let mut results = vec![];
    results.push(compile_file(&f_today, &ctx(app, source.clone())).await?);

    if ctx.scheduler.self_heal {
        results.push(self_heal(&f_prev, host_slug(), app).await?);
    }

    commit_push(&touched_files(), dailies_dir()).await?;

    for r in &results { emit(app, "pipeline-done", r)?; }
    Ok(())
}
```

- **Lock**: `mkdir + PID + 10min stale` (see [Concurrency](#concurrency)).
- **git sync**: `git pull --rebase --autostash` on the dailies repo. Failures log a warning but do not abort (the local copy is still usable).
- **Targets**: `(F_TODAY, F_PREV)` where `F_PREV` is the previous business day from `F_TODAY`.

### 2. `compute_targets(today: Date)` → `(F_TODAY, F_PREV)`

```rust
pub fn compute_targets(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let f_today = next_business_day(today);
    let f_prev  = previous_business_day(f_today);
    (f_today, f_prev)
}
```

`F_TODAY` = next business day after today. `F_PREV` = previous business day from `F_TODAY`. (See `docs/specs/standup-file-format.md` for `next_business_day`.)

### 3. `compile_file(F, ctx)` — the core compile for one file

This is the heart of the pipeline. Each sub-step is its own function so it can be unit-tested in isolation and so `preview_gather` can stop after step (e).

| Sub-step | Function | Output |
| --- | --- | --- |
| a | `compute_window(F)` | `(range_start, range_end, dates[])` |
| b | `gather_git_facts(window, config)` | per-repo git log, file scope, areas |
| c | `gather_notes(window, dates, github_dir)` | today-*.md/.done.md + now.md |
| d | `anti_regression_guard(F, host)` | `Skip` if FACTS empty but last run had repos |
| e | `gather_enrichment(window, config)` | cached: CONV, PRREV, GITHUB, CLAUDEFILES, OPENCODE, CODEX, GEMINI, GROK |
| f | `compute_provenance(facts, notes, all_git_tickets)` | `(FORBIDDEN, COVERED, SKEW)` |
| g | `scrub_notes(notes, forbidden, covered, meta)` | `clean_notes` |
| h | `scrub_enrichment(conv, ...)` | `clean_conv` |
| i | `redact(facts, clean_notes, clean_conv, github)` | redacted inputs |
| j | `dirty_check(F, host, hash(inputs))` | `Skip` \| `Continue` |
| k | `read_existing(F, host)` | `(manual_region, prev_auto)` |
| l | `render_det(facts, github, notes, conv, prrev)` | `det_body` (always produced) |
| m | `render_llm(inputs, provider)` | `Option<body>` (per render_mode; CLI-first → API fallback) |
| n | `validate_render(llm_body, facts)` | `Ok(body)` \| `Err` → use `det_body` |
| o | `accumulate(new_body, prev_auto)` | `final_body` (re-inject uncovered PREV bullets) |
| p | `redact(final_body)` | `clean_body` |
| q | `write_audit(F, host, inputs, render_info)` | sidecar JSON |
| r | `write_file(F, host, title, subtitle, clean_body)` | atomic write |
| s | `persist_hash(F, host, hash)` | only if clean LLM render |

#### (a) `compute_window(F)`

```rust
pub fn compute_window(f: NaiveDate) -> Window {
    let range_end   = previous_business_day(f);     // last work day before F
    let range_start = previous_business_day(range_end); // the work day before that
    let dates = business_days_between(range_start, range_end);
    Window { range_start, range_end, dates }
}
```

The window is the **two-business-day** range ending the day before `F`. So if `F = Monday`, the window is the prior Thu–Fri (or earlier if holidays, but autostand doesn't model holidays).

#### (b) `gather_git_facts`

For each repo under `github_dir` (filtered by `standup_authors`), run `git log <git_refs> --since=<range_start> --until=<range_end + 1day> --author=<author>` and collect commits, tickets (parsed from subject), files, and areas (top-level dirs). Result: `Vec<RepoFacts>`.

#### (c) `gather_notes`

Scan `github_dir` for `today-*.md`, `.done.md`, and `now.md` files dated within the window. Each note file produces a `NoteRef { source, date, clauses }` where clauses are the bullet lines.

#### (d) `anti_regression_guard`

If `facts.is_empty()` but the last audit sidecar for `F`/host shows repos were present, return `Skip` — this prevents the standup from going empty due to a transient git/network failure. Logs a warning; emits `pipeline-error` with `code: "anti_regression"` but `CompileResult.status = "skip"`.

#### (e) `gather_enrichment`

Cached gathering of enrichment sources. Each source is cached by `(source, window_hash)` with a 2700s TTL (see [Cache](#cache)). Sources:

- `CONV` — Claude Code conversation digests (`~/.claude/...`)
- `PRREV` — GitHub PR reviews by `review.reviewer` in `review.pr_org` (via `gh`)
- `GITHUB` — GitHub PRs/issues touched in the window
- `CLAUDEFILES` — files Claude Code wrote
- `OPENCODE` — opencode sessions (`~/.local/share/opencode/...`)
- `CODEX` — Codex sessions
- `GEMINI` — Gemini CLI sessions
- `GROK` — Grok CLI sessions

Only sources enabled in `data_sources` are gathered. Only **Ok** results are cached (errors re-fetch next run).

#### (f) `compute_provenance`

See `docs/specs/anti-backdating.md` for the full algorithm. Outputs:

- `FORBIDDEN` — note tickets whose commits are on a day NOT in the window (cross-day backdating).
- `COVERED` — note tickets already in `range_tickets` or GITHUB (don't duplicate from notes).
- `SKEW` — notes dated in-range naming a ticket whose commits ALL fall outside the window.

#### (g) `scrub_notes`

Drop note clauses that: match the CLAIM regex (assert committed work), name a FORBIDDEN ticket, name a COVERED ticket, or match the META regex. See `docs/specs/anti-backdating.md`.

#### (h) `scrub_enrichment`

Same scrub applied to CONV and PRREV text. Removes META references and any FORBIDDEN ticket mentions that would re-introduce backdated claims.

#### (i) `redact`

Apply redaction to all inputs before sending to the LLM: strip absolute paths to repo-relative, redact any tokens matching `meta_extra`, and redact email addresses. Produces the `inputs` struct that `render_llm` consumes.

#### (j) `dirty_check`

Hash the redacted inputs and compare to the persisted hash for `F`/host (in `state/hashes/<F>-<host>.txt`). If the hash matches and the prior render was a clean LLM render, `Skip` — the file is already up to date. Otherwise `Continue`.

#### (k) `read_existing`

Parse the existing `dailies/<F>.md` (if any) into `(manual_region, prev_auto)` where `prev_auto` is the AUTO block body for this host. Used by `accumulate`.

#### (l) `render_det`

Always produced. A deterministic template render of the facts, github, notes, conv, and prrev. This is the fallback if LLM fails. Never skipped.

#### (m) `render_llm`

Per `render_mode`:

- `Det` → return `None` (no LLM call).
- `Llm` → call the preferred provider; on failure, return `None` (no fallback).
- `Auto` → call the preferred provider in `CliFirst` mode: try CLI, fall back to API. On failure, return `None` (caller will use `det_body`).

The provider is chosen from `llm.preferred_provider` (or `AUTOSTAND_LLM_PROVIDER` env). CLI-first means: if `claude` CLI is detected and enabled, invoke `claude` as a subprocess; if that fails or the CLI is absent, use the API.

#### (n) `validate_render`

Validate the LLM output against the facts: every ticket mentioned must appear in `range_tickets`; no FORBIDDEN tickets introduced; bullets are past-tense; length within bounds. On failure, use `det_body` and set `fellback = true`.

#### (o) `accumulate`

Re-inject any PREV bullet not covered by the new render. Uses `textsim::covered` (significant-word overlap threshold). See `docs/specs/anti-backdating.md` — **accumulate never deletes**.

#### (p) `redact`

Final redaction pass on `final_body` (in case the LLM reintroduced absolute paths or emails).

#### (q) `write_audit`

Write the audit sidecar JSON to `state/audit/<F>-<host>.json` with permissions `0600`, atomic write. See `docs/specs/audit.md`.

#### (r) `write_file`

Atomic write of the final standup file: read existing → insert/replace this host's AUTO block before MANUAL:START → atomic write (temp → fsync → rename) with mode `0600`.

#### (s) `persist_hash`

Only if the LLM render was clean (not a fallback). Write `hash` to `state/hashes/<F>-<host>.txt`. This is what makes the next `dirty_check` skip a re-render.

### 4. `self_heal(F_PREV, host)`

If `F_PREV != F_TODAY` (the day has rolled over) and `F_PREV`'s AUTO block for this host is empty, compile it from durable disk data. Durable = git log + notes files still on disk. Skip if `F_PREV` is **frozen** (AUTO block already populated — see `docs/specs/anti-backdating.md`).

```rust
pub async fn self_heal(f_prev: &NaiveDate, host: &str, app: &AppHandle) -> Result<CompileResult> {
    let existing = parse_file(&dailies_dir().join(format!("{}.md", f_prev)));
    if existing.auto_for(host).map(|b| !b.body.is_empty()).unwrap_or(false) {
        return Ok(CompileResult::skip(f_prev, host, "frozen"));
    }
    compile_file(f_prev, &ctx(app, TriggerSource::SelfHeal)).await
}
```

### 5. `commit_push(touched_files)`

```rust
pub async fn commit_push(files: &[PathBuf], dir: &Path) -> Result<()> {
    if files.is_empty() { return Ok(()); }
    // skip marker files (state/, audit/, hashes/) — these are not committed
    let standup_files: Vec<_> = files.iter()
        .filter(|p| p.extension() == Some(OsStr::new("md")))
        .collect();
    if standup_files.is_empty() { return Ok(()); }

    tokio::task::spawn_blocking(move || {
        let dates = standup_files.iter()
            .filter_map(|p| p.file_stem()?.to_str())
            .collect::<Vec<_>>().join(", ");
        std::process::Command::new("git").arg("add")
            .args(standup_files.iter().map(|p| p.as_os_str()))
            .current_dir(dir).status()?;
        std::process::Command::new("git").arg("commit")
            .arg("-m").arg(format!("standup: {dates}"))
            .current_dir(dir).status()?;
        std::process::Command::new("git").arg("push")
            .current_dir(dir).status()?;
        Ok::<_, anyhow::Error>(())
    }).await??;
    Ok(())
}
```

- Only `.md` files are committed; `state/`, `audit/`, `hashes/` are gitignored or never staged.
- Commit message: `standup: <comma-separated dates>`.
- `git push` failure logs a warning but does not abort — the commit is local and will push on the next successful run.

---

## Cache

`gather_enrichment` uses a TTL cache keyed by `(source, window_hash)`. Only **Ok** results are cached; errors re-fetch next run.

| Property | Value |
| --- | --- |
| Backend | JSON files in `state/cache/<source>/<window_hash>.json` |
| TTL | 2700 seconds (45 min) |
| Key | `(source_id, sha256(window.range_start..range_end))` |
| Invalidation | TTL expiry or `set_config` (any change clears the whole cache) |
| Permissions | `0600` (may contain redacted conversation digests) |

```rust
pub struct Cache {
    dir: PathBuf,
    ttl: Duration,
}

impl Cache {
    pub async fn get<T: DeserializeOwned>(&self, source: &str, window: &Window) -> Option<T> {
        let path = self.path_for(source, window);
        let meta = std::fs::metadata(&path).ok()?;
        if meta.modified().ok()?.elapsed().ok()? > self.ttl { return None; }
        let bytes = std::fs::read(&path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
    pub fn put<T: Serialize>(&self, source: &str, window: &Window, val: &T) -> Result<()> {
        let path = self.path_for(source, window);
        std::fs::create_dir_all(path.parent().unwrap())?;
        let bytes = serde_json::to_vec(val)?;
        let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        tmp.write_all(&bytes)?;
        tmp.as_file().sync_all()?;
        tmp.persist(&path)?;
        set_mode_0600(&path)?;
        Ok(())
    }
}
```

---

## Concurrency

A single-run lock prevents two compiles from racing on the same machine.

- **Acquire**: `mkdir state/lock/` (atomic). Write current PID + timestamp to `state/lock/pid`.
- **Stale**: if the lock dir exists and the PID inside is not running, or the timestamp is older than 10 minutes, steal the lock.
- **Release**: `rmdir state/lock/`.

```rust
const LOCK_TIMEOUT: Duration = Duration::from_secs(600); // 10 min

pub async fn acquire_lock(state: &Path, timeout: Duration) -> Result<LockGuard> {
    let lock = state.join("lock");
    for _ in 0..(timeout.as_secs()) {
        match std::fs::create_dir(&lock) {
            Ok(()) => {
                std::fs::write(lock.join("pid"), std::process::id().to_string())?;
                return Ok(LockGuard { path: lock });
            }
            Err(_) => {
                if let Ok(pid_str) = std::fs::read_to_string(lock.join("pid")) {
                    if let Ok(pid) = pid_str.trim().parse::<i32>() {
                        if !is_running(pid) { steal(&lock)?; continue; }
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
    Err(anyhow!("lock timeout"))
}
```

**No parallel compiles.** A `trigger` call while another is running returns `Err(AppError::Lock)` immediately; the scheduler does not queue.

---

## Error recovery

Any step failure logs to the run log + emits a `pipeline-error` event. The **deterministic fallback** ensures a standup is always produced if any facts/notes exist:

| Step failure | Behavior |
| --- | --- |
| `gather_git_facts` fails for one repo | skip that repo; continue with others |
| `gather_enrichment` fails for one source | skip that source; record in audit sidecar; continue |
| `render_llm` fails (CLI missing, API error, timeout) | use `det_body`; `fellback = true`; `render_used = "llm_fallback"` |
| `validate_render` rejects LLM body | use `det_body`; `fellback = true` |
| `write_file` fails | abort `compile_file`; `CompileResult.status = "error"` |
| `git pull`/`git push` fails | log warning; continue (local copy is authoritative) |
| `acquire_lock` fails | abort `trigger`; emit `pipeline-error` with `code: "lock"` |

The run log lives at `<state>/runs/<F>-<host>-<timestamp>.log` and is rotated weekly.

---

## Mermaid flowchart

```mermaid
flowchart TD
    A["trigger(source)"] --> B["acquire_lock (mkdir+PID)"]
    B --> C["git_sync_pull"]
    C --> D["compute_targets(today) → (F_TODAY, F_PREV)"]
    D --> E["compile_file(F_TODAY, ctx)"]
    D --> F["self_heal(F_PREV, host)"]
    E --> G["commit_push(touched_files)"]
    F --> G
    G --> H["release_lock"]

    subgraph compile_file[F]
        CA["a: compute_window(F)"] --> CB["b: gather_git_facts"]
        CB --> CC["c: gather_notes"]
        CC --> CD{"d: anti_regression_guard<br/>FACTS empty?"}
        CD -- "yes, last run had repos" --> SKIP["Skip"]
        CD -- "no" --> CE["e: gather_enrichment (cached, TTL=2700s)"]
        CE --> CF["f: compute_provenance → FORBIDDEN, COVERED, SKEW"]
        CF --> CG["g: scrub_notes"]
        CG --> CH["h: scrub_enrichment"]
        CH --> CI["i: redact(inputs)"]
        CI --> CJ{"j: dirty_check<br/>hash matches?"}
        CJ -- "yes, clean LLM last run" --> SKIP
        CJ -- "no" --> CK["k: read_existing → (manual_region, prev_auto)"]
        CK --> CL["l: render_det → det_body (always)"]
        CL --> CM{"m: render_llm<br/>per render_mode"}
        CM -- "Det / fail" --> CN["n: validate_render → det_body"]
        CM -- "Auto/Llm ok" --> CN
        CN --> CO["o: accumulate(new, prev_auto) → final_body"]
        CO --> CP["p: redact(final_body)"]
        CP --> CQ["q: write_audit(sidecar JSON, 0600)"]
        CQ --> CR["r: write_file(atomic, 0600)"]
        CR --> CS{"s: persist_hash<br/>clean LLM render?"}
        CS -- "yes" --> CT["write hash"]
        CS -- "no" --> CU["skip persist"]
        CT --> DONE["CompileResult"]
        CU --> DONE
        SKIP --> DONE
    end
```

The flowchart matches the function names in `crates/autostand-core/src/pipeline.rs` 1:1. Every box is a `pub` function; arrows are call order. The two `Skip` exits return `CompileResult { status: "skip", .. }` without writing anything.