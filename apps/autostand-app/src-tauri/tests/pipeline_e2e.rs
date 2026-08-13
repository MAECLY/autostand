//! End-to-end proof that the F4 pipeline wiring produces a real standup.
//!
//! Everything here is hermetic: three `tempfile::TempDir`s stand in for
//! `github_dir`, the dailies git working tree and the autostand state dir. The
//! only subprocess is `git`, and it only ever runs against those temp repos —
//! no `gh`, no LLM CLI, no network, no dependence on the developer's `HOME`.
//!
//! # Why the pipeline is driven headlessly
//!
//! The production entry points — `pipeline_runner::trigger` and
//! `pipeline_runner::compile_one` — take a `tauri::AppHandle`, which they use for
//! three things: `RunEnv::load` reads the config out of the Tauri store,
//! `commands::state_dir()` resolves the *platform* state dir, and every step
//! emits a `pipeline-*` event. All three would drag the developer's real machine
//! into the test, and a mock `AppHandle` cannot fix the first two.
//!
//! So [`compile_headless`] walks the same steps (a…s) of
//! `docs/specs/pipeline.md`, in the same order, through the same `pub` functions
//! `compile_one` calls — `gather::gather_all`, `pipeline_runner::{split_ticket_days,
//! is_regression, inputs_hash, should_skip_unchanged, decide_render, audit_patch,
//! ok_result, skip_result}`, `provenance::compute`, `hashes::{is_unchanged,
//! persist_hash}` and `autostand_core::pipeline::compile_file`. What it replaces
//! is only the `AppHandle`-bound plumbing: the config lookup, the platform state
//! dir, and the event emission. Two small private helpers of `pipeline_runner`
//! (`read_auto_body` and `enrich_sidecar`) are mirrored here as [`auto_body`] and
//! [`merge_json`] because they are not reachable from an integration test.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::NaiveDate;
use tempfile::TempDir;

use autostand_app::commands::types::{
    AppConfig, CompileResult, CompileStatus, DataSourceConfigs, RenderMode, RenderUsed,
};
use autostand_app::gather::{self, Gathered};
use autostand_app::git_ops;
use autostand_app::pipeline_runner::{self, RenderDecision};
use autostand_core::dates::{self, Window};
use autostand_core::pipeline::{self as core_pipeline, CompileInputs};
use autostand_core::provenance::{self, Provenance};
use autostand_core::{fileops, format, hashes};
use autostand_scheduler::triggers::TriggerSource;

// ── fixture constants ─────────────────────────────────────────────────────

/// Host slug that owns this machine's AUTO block in every test below.
const HOST: &str = "e2e-host";

/// A second host whose AUTO block must survive every compile untouched.
const FOREIGN_HOST: &str = "other-host";

/// Name of the work repo planted under the temp `github_dir`.
const WORK_REPO: &str = "acme-api";

/// Author of the out-of-window commit and of `FIF-201`.
const AUTHOR_ONE: &str = "Dev One <one@example.invalid>";

/// Author of `FIF-202`. Kept separate so a test can narrow the author filter and
/// make an already-rendered bullet disappear from the FACTS.
const AUTHOR_TWO: &str = "Dev Two <two@example.invalid>";

/// `AUTHOR_ONE`'s email, as it appears in `standup_authors`.
const AUTHOR_ONE_EMAIL: &str = "one@example.invalid";

/// `AUTHOR_TWO`'s email, as it appears in `standup_authors`.
const AUTHOR_TWO_EMAIL: &str = "two@example.invalid";

/// Jira base URL the deterministic renderer links tickets against.
const JIRA_BASE: &str = "https://jira.example.invalid/browse";

/// Hand-added MANUAL item seeded before a compile; auto-compile must never touch it.
const MANUAL_ITEM: &str = "- Booked the Q4 planning offsite with the platform team";

/// A populated AUTO block belonging to another machine.
const FOREIGN_AUTO: &str = "**laptop-repo — nightly export**\n- Tuned the nightly export job";

/// The narrative note gathered for `2026-08-03`.
///
/// The second clause names `FIF-900`, whose only commit landed on `2026-07-20` —
/// outside the window. Anti-backdating must drop that clause and keep the first.
const NOTES: &str = "- Paired with design on the onboarding walkthrough\n\
                     - Kept iterating on the FIF-900 rollout plan\n";

// ── dates ─────────────────────────────────────────────────────────────────

/// Build a date, panicking on an invalid one.
fn d(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
}

/// The filing date every test compiles: Tuesday 2026-08-04.
///
/// Its window is Fri 2026-07-31 … Mon 2026-08-03, which straddles both a weekend
/// and a month boundary — the two cases the subtitle wording has to get right.
fn filing_date() -> NaiveDate {
    d(2026, 8, 4)
}

// ── git helpers ───────────────────────────────────────────────────────────

/// Run `git <args>` in `dir`, asserting success, and return trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    git_env(dir, args, &[])
}

/// [`git`] with extra environment variables (used to pin commit dates).
fn git_env(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> String {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(dir);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// `git init` a throwaway repo with a local identity and every ambient influence
/// (signing, hooks, commit template) disabled.
///
/// WHY: the assertions below must not depend on the developer's global git
/// configuration — a `commit.gpgsign = true` or a `commit.template` in `~/.gitconfig`
/// would otherwise change what lands in the commit message.
fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).expect("repo dir");
    git(dir, &["init", "--quiet"]);
    // Version-agnostic equivalent of `git init -b main`.
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(dir, &["config", "user.email", "fixture@example.invalid"]);
    git(dir, &["config", "user.name", "Autostand Fixture"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    git(dir, &["config", "commit.template", ""]);
    let hooks = dir.join(".empty-hooks");
    fs::create_dir_all(&hooks).expect("hooks dir");
    git(
        dir,
        &["config", "core.hooksPath", &hooks.display().to_string()],
    );
}

/// Commit `subject` to `repo`, authored by `author` at `when`.
///
/// `when` is a naive local timestamp (`YYYY-MM-DD HH:MM:SS`) written to both
/// `GIT_AUTHOR_DATE` and `GIT_COMMITTER_DATE`: `git log --since/--until` filters on
/// the *committer* date, so pinning only the author date would leave the window
/// boundary at the mercy of the wall clock.
fn commit(repo: &Path, author: &str, when: &str, file: &str, subject: &str) {
    fs::write(repo.join(file), format!("// {subject}\n")).expect("write source file");
    git(repo, &["add", "--", file]);
    git_env(
        repo,
        &[
            "commit",
            "--quiet",
            "--author",
            author,
            "--message",
            subject,
        ],
        &[("GIT_AUTHOR_DATE", when), ("GIT_COMMITTER_DATE", when)],
    );
}

// ── fixture ───────────────────────────────────────────────────────────────

/// A complete, hermetic autostand installation on disk.
struct Fixture {
    /// Stands in for `github_dir`; holds the scanned work repo and its notes.
    github: TempDir,
    /// The dailies git working tree. Standup files live in `<root>/dailies`.
    dailies: TempDir,
    /// The autostand state dir (audit sidecars, hashes, gather cache).
    state: TempDir,
    /// The persisted app config, as `RunEnv::load` would have produced it.
    config: AppConfig,
    /// The resolved host slug.
    host: String,
}

impl Fixture {
    /// The autostand state dir.
    fn state_dir(&self) -> PathBuf {
        self.state.path().to_path_buf()
    }

    /// The directory `<date>.md` files are written to.
    fn dailies_dir(&self) -> PathBuf {
        self.dailies.path().join("dailies")
    }

    /// The git working tree that contains [`Fixture::dailies_dir`].
    fn repo_dir(&self) -> PathBuf {
        self.dailies.path().to_path_buf()
    }

    /// The scanned work repo under `github_dir`.
    fn work_repo(&self) -> PathBuf {
        self.github.path().join(WORK_REPO)
    }

    /// Path of the standup file for `date`.
    fn file_path(&self, date: NaiveDate) -> PathBuf {
        self.dailies_dir().join(format!("{date}.md"))
    }
}

/// Build the fixture: one work repo with three commits and one note file, plus an
/// empty dailies repo and an empty state dir.
///
/// Only `local-git` and `remember` are enabled. Every other source resolves its
/// availability from the ambient environment (`gh` on `PATH`, `~/.claude`, …), so
/// leaving them on would make the test depend on the developer's machine.
fn fixture() -> Fixture {
    let github = TempDir::new().expect("github temp dir");
    let dailies = TempDir::new().expect("dailies temp dir");
    let state = TempDir::new().expect("state temp dir");

    let work = github.path().join(WORK_REPO);
    init_repo(&work);
    // Outside the window (2026-07-20 is a Monday, eleven days before range_start).
    commit(
        &work,
        AUTHOR_ONE,
        "2026-07-20 12:00:00",
        "billing.rs",
        "feat(billing): FIF-900 import legacy invoices",
    );
    // range_start (Friday) and range_end (Monday).
    commit(
        &work,
        AUTHOR_ONE,
        "2026-07-31 12:00:00",
        "checkout.rs",
        "fix(checkout): FIF-201 correct total rounding",
    );
    commit(
        &work,
        AUTHOR_TWO,
        "2026-08-03 12:00:00",
        "inventory.rs",
        "feat(inventory): FIF-202 retry failed syncs",
    );
    // Past range_end: work done on the filing date belongs to the *next* standup.
    commit(
        &work,
        AUTHOR_TWO,
        "2026-08-04 09:00:00",
        "shipping.rs",
        "feat(shipping): FIF-950 add an express tier",
    );

    let notes_dir = work.join(".remember");
    fs::create_dir_all(&notes_dir).expect("remember dir");
    fs::write(notes_dir.join("today-2026-08-03.md"), NOTES).expect("write note");

    init_repo(dailies.path());
    fs::create_dir_all(dailies.path().join("dailies")).expect("dailies dir");

    let config = AppConfig {
        github_dir: github.path().display().to_string(),
        dailies_dir: dailies.path().join("dailies").display().to_string(),
        standup_authors: vec![AUTHOR_ONE_EMAIL.to_string(), AUTHOR_TWO_EMAIL.to_string()],
        git_refs: "--all".to_string(),
        jira_base: JIRA_BASE.to_string(),
        host_slug_override: Some(HOST.to_string()),
        render_mode: RenderMode::Det,
        data_sources: DataSourceConfigs {
            local_git: true,
            remember: true,
            github: false,
            claude_code: false,
            opencode: false,
            codex: false,
            gemini_cli: false,
            grok_cli: false,
        },
        ..AppConfig::default()
    };
    let host = pipeline_runner::resolve_host(&config, state.path());
    assert_eq!(host, HOST, "a valid override must win over detection");

    Fixture {
        github,
        dailies,
        state,
        config,
        host,
    }
}

/// Seed `<date>.md` with another machine's populated AUTO block and one hand-added
/// MANUAL item, using the production writers.
fn seed_existing_file(fx: &Fixture, date: NaiveDate) {
    let path = fx.file_path(date);
    fileops::set_auto(
        &path,
        FOREIGN_HOST,
        "August 04, 2026",
        "_Work completed July 31–August 03, 2026._",
        FOREIGN_AUTO,
    )
    .expect("seed foreign AUTO block");
    fileops::add_manual(&path, MANUAL_ITEM).expect("seed MANUAL item");
}

// ── headless pipeline ─────────────────────────────────────────────────────

/// What one headless compile produced, plus the intermediates the assertions
/// need in order to reason about *why* the file looks the way it does.
struct RunOutcome {
    /// The IPC result `compile_one` would have returned.
    result: CompileResult,
    /// Step (a): the gather window.
    window: Window,
    /// The FACTS block, with the `TICKET_DAYS` marker already split out.
    facts: String,
    /// Step (f): the `FORBIDDEN` / `COVERED` / `SKEW` verdict.
    provenance: Provenance,
    /// Step (j): the digest of the gathered inputs.
    hash: String,
}

/// The per-run values [`core_inputs`] needs beyond the fixture itself.
struct CoreInput<'a> {
    fx: &'a Fixture,
    date: NaiveDate,
    window: &'a Window,
    facts: &'a str,
    gathered: &'a Gathered,
    provenance: &'a Provenance,
    prev_auto: String,
    decision: &'a RenderDecision,
}

/// Assemble the pure-core inputs exactly as `compile_one` does.
fn core_inputs(input: CoreInput<'_>) -> CompileInputs {
    let CoreInput {
        fx,
        date,
        window,
        facts,
        gathered,
        provenance,
        prev_auto,
        decision,
    } = input;
    CompileInputs {
        facts: facts.to_string(),
        notes: gathered.notes.clone(),
        github: gathered.github.clone(),
        conv: gathered.conv.clone(),
        prrev: gathered.prrev.clone(),
        claude_files: gathered.claude_files.clone(),
        prev_auto_body: prev_auto,
        forbidden_tickets: provenance.forbidden_tickets.clone(),
        covered_tickets: provenance.covered_tickets.clone(),
        ticket_days: provenance.ticket_days.clone(),
        skew: provenance.skew.clone(),
        jira_base: fx.config.jira_base.clone(),
        render_mode: decision.core_mode(),
        llm_body: decision.llm_body().map(str::to_string),
        host: fx.host.clone(),
        state_dir: fx.state_dir(),
        dailies_dir: fx.dailies_dir(),
        file_date: date,
        title: pipeline_runner::title_for(date),
        subtitle: pipeline_runner::subtitle_for(window),
    }
}

/// `pipeline_runner::compile_one` with the `AppHandle` removed — see the module docs.
async fn compile_headless(fx: &Fixture, date: NaiveDate, source: TriggerSource) -> RunOutcome {
    let file_path = fx.file_path(date);

    // (a) window
    let window = dates::compute_window(date);

    // (b, c, e) gather
    let gathered = gather::gather_all(
        &gather::to_date_window(&window),
        &fx.config,
        &fx.state_dir(),
    )
    .await;
    assert!(
        gathered.failures.is_empty(),
        "a hermetic gather must not fail: {:?}",
        gathered.failures
    );
    let (facts, all_git_tickets) = pipeline_runner::split_ticket_days(&gathered.facts);

    // (d) anti-regression guard
    let previous_repos = previous_repo_count(&fx.state_dir(), date, &fx.host);
    assert!(
        !pipeline_runner::is_regression(&facts, previous_repos),
        "the fixture always has commits, so the guard must never fire"
    );

    // (f) provenance
    let provenance = provenance::compute(
        &facts,
        &gathered.notes,
        gathered.github.as_deref(),
        &all_git_tickets,
        &window.dates,
    );

    // (j) dirty check
    let hash = pipeline_runner::inputs_hash(&facts, &gathered);
    let unchanged = hashes::is_unchanged(&fx.state_dir(), date, &fx.host, &hash);
    if pipeline_runner::should_skip_unchanged(source, unchanged) {
        let result = pipeline_runner::skip_result(date, &fx.host, &file_path, "inputs unchanged");
        return RunOutcome {
            result,
            window,
            facts,
            provenance,
            hash,
        };
    }

    // (k) this host's existing AUTO body
    let prev_auto = auto_body(&file_path, &fx.host).unwrap_or_default();

    // (m, n) `Det` short-circuits before any provider is contacted.
    let decision = pipeline_runner::decide_render(
        RenderMode::Det,
        None,
        &provenance.range_tickets,
        &provenance.forbidden_tickets,
    );

    // (g…r) scrub, redact, render, accumulate, redact, atomic write, sidecar.
    fs::create_dir_all(fx.dailies_dir()).expect("dailies dir");
    let outputs = core_pipeline::compile_file(&core_inputs(CoreInput {
        fx,
        date,
        window: &window,
        facts: &facts,
        gathered: &gathered,
        provenance: &provenance,
        prev_auto,
        decision: &decision,
    }))
    .expect("compile_file writes the standup");

    // (q) patch in what the pure core could not know.
    if let Some(path) = outputs.audit_path.as_ref() {
        let overlay =
            pipeline_runner::audit_patch(&window, &facts, &gathered, &decision, None, &[], &hash);
        merge_json(path, &overlay);
    }

    // (s) a deterministic render never earns a persisted hash.
    if decision.persists_hash() {
        hashes::persist_hash(&fx.state_dir(), date, &fx.host, &hash).expect("persist hash");
    }

    let result = pipeline_runner::ok_result(
        date,
        &fx.host,
        &file_path,
        &decision,
        &outputs,
        pipeline_runner::outcome_message(&decision, None),
    );
    RunOutcome {
        result,
        window,
        facts,
        provenance,
        hash,
    }
}

// ── helpers mirroring `pipeline_runner`'s private functions ───────────────

/// Path of the audit sidecar for `(date, host)`.
fn sidecar_path(state_dir: &Path, date: NaiveDate, host: &str) -> PathBuf {
    state_dir.join("audit").join(format!("{date}-{host}.json"))
}

/// How many repos the previous run recorded for `(date, host)`; 0 when absent.
fn previous_repo_count(state_dir: &Path, date: NaiveDate, host: &str) -> usize {
    let Ok(bytes) = fs::read(sidecar_path(state_dir, date, host)) else {
        return 0;
    };
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .as_ref()
        .map_or(0, pipeline_runner::sidecar_repo_count)
}

/// `host`'s AUTO body in the file at `path`, or `None` when there is no block for it.
fn auto_body(path: &Path, host: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let parsed = format::parse(&content).ok()?;
    parsed
        .auto_blocks
        .into_iter()
        .find(|block| block.host == host)
        .map(|block| block.body)
}

/// Merge `overlay`'s top-level keys into the JSON document at `path`.
fn merge_json(path: &Path, overlay: &serde_json::Value) {
    let bytes = fs::read(path).expect("read sidecar");
    let mut doc: serde_json::Value = serde_json::from_slice(&bytes).expect("sidecar is json");
    let target = doc.as_object_mut().expect("sidecar is a json object");
    for (key, value) in overlay.as_object().expect("patch is a json object") {
        target.insert(key.clone(), value.clone());
    }
    fs::write(
        path,
        serde_json::to_string_pretty(&doc).expect("serialize sidecar"),
    )
    .expect("write sidecar");
}

/// Read a JSON array of strings, panicking if it is not one.
fn strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .expect("json array")
        .iter()
        .map(|item| item.as_str().expect("json string").to_string())
        .collect()
}

/// Read and parse the standup file for `date`.
fn read_file(fx: &Fixture, date: NaiveDate) -> String {
    fs::read_to_string(fx.file_path(date)).expect("standup file exists")
}

// ── tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn compiles_a_real_repo_into_a_well_formed_standup() {
    let fx = fixture();
    let date = filing_date();
    seed_existing_file(&fx, date);

    let run = compile_headless(&fx, date, TriggerSource::Manual).await;

    assert_eq!(run.result.status, CompileStatus::Ok);
    assert_eq!(run.result.render_used, RenderUsed::Det);
    assert!(!run.result.fellback);
    assert_eq!(run.result.date, "2026-08-04");
    assert_eq!(run.result.host, HOST);
    assert_eq!(run.window.range_start, d(2026, 7, 31));
    assert_eq!(run.window.range_end, d(2026, 8, 3));

    let body = read_file(&fx, date);

    // Title + subtitle per `docs/specs/standup-file-format.md`.
    assert!(
        body.contains("# Daily Standup — August 04, 2026"),
        "title missing: {body}"
    );
    assert!(
        body.contains("_Work completed July 31–August 03, 2026._"),
        "subtitle missing or misworded: {body}"
    );

    // A well-formed AUTO block for this host.
    assert!(
        body.contains(&format!("<!-- AUTO:{HOST}:START -->")),
        "{body}"
    );
    assert!(
        body.contains(&format!("<!-- AUTO:{HOST}:END -->")),
        "{body}"
    );
    assert!(body.contains("<!-- MANUAL:START -->"), "{body}");
    assert!(body.contains("<!-- MANUAL:END -->"), "{body}");

    let parsed = format::parse(&body).expect("the written file parses");
    let mine = parsed
        .auto_for(HOST)
        .expect("an AUTO block for this host")
        .body
        .clone();

    // Both in-window commits reached the body, with their Jira links.
    assert!(mine.contains("acme-api"), "{mine}");
    assert!(
        mine.contains(&format!("[FIF-201]({JIRA_BASE}/FIF-201)")),
        "{mine}"
    );
    assert!(mine.contains("correct total rounding"), "{mine}");
    assert!(
        mine.contains(&format!("[FIF-202]({JIRA_BASE}/FIF-202)")),
        "{mine}"
    );
    assert!(mine.contains("retry failed syncs"), "{mine}");
    assert!(
        mine.contains("Paired with design on the onboarding walkthrough"),
        "the honest note clause is missing: {mine}"
    );

    // Neither out-of-window commit appears anywhere in the file: FIF-900 landed
    // before range_start, FIF-950 after range_end (on the filing date itself).
    assert!(!body.contains("FIF-900"), "backdated ticket leaked: {body}");
    assert!(
        !body.contains("import legacy invoices"),
        "a commit from before the window leaked: {body}"
    );
    assert!(
        !body.contains("FIF-950"),
        "forward-dated ticket leaked: {body}"
    );
    assert!(
        !body.contains("express tier"),
        "a commit from after the window leaked: {body}"
    );

    // The MANUAL region and the other machine's AUTO block are untouched.
    assert_eq!(parsed.manual.body.trim(), MANUAL_ITEM);
    assert_eq!(
        parsed
            .auto_for(FOREIGN_HOST)
            .expect("the other host's block survives")
            .body
            .trim(),
        FOREIGN_AUTO
    );
}

#[tokio::test]
async fn writes_an_audit_sidecar_with_the_real_window_and_facts() {
    let fx = fixture();
    let date = filing_date();

    let run = compile_headless(&fx, date, TriggerSource::Manual).await;

    let reported = run
        .result
        .audit_path
        .as_deref()
        .expect("audit path reported");
    let expected = sidecar_path(&fx.state_dir(), date, HOST);
    assert_eq!(Path::new(reported), expected);

    let doc: serde_json::Value =
        serde_json::from_slice(&fs::read(&expected).expect("read sidecar")).expect("sidecar json");

    assert_eq!(doc["file"], "2026-08-04");
    assert_eq!(doc["host"], HOST);
    assert_eq!(doc["render_mode"], "det");
    assert_eq!(doc["render_used"], "det");
    assert_eq!(doc["fellback"].as_bool(), Some(false));
    assert_eq!(doc["hash"].as_str(), Some(run.hash.as_str()));

    // The real gather window — not core's `F..F` placeholder, which would make
    // `audit::classify` call every honest bullet `unverified`.
    assert_eq!(doc["window"]["start"], "2026-07-31");
    assert_eq!(doc["window"]["end"], "2026-08-03");

    // Provenance.
    assert_eq!(strings(&doc["covered_tickets"]), ["FIF-201", "FIF-202"]);
    assert_eq!(strings(&doc["forbidden_tickets"]), ["FIF-900"]);
    assert_eq!(strings(&doc["ticket_days"]["FIF-201"]), ["2026-07-31"]);
    assert_eq!(strings(&doc["ticket_days"]["FIF-202"]), ["2026-08-03"]);
    assert_eq!(strings(&doc["ticket_days"]["FIF-900"]), ["2026-07-20"]);
    assert_eq!(doc["skew"][0]["ticket"], "FIF-900");
    assert_eq!(doc["skew"][0]["note_date"], "2026-08-03");

    // Gathered facts, as the async layer patched them in.
    let facts = doc["facts"].as_array().expect("facts array");
    assert_eq!(facts.len(), 1, "one repo was scanned: {facts:?}");
    assert_eq!(facts[0]["repo"], WORK_REPO);
    assert_eq!(
        facts[0]["commits"].as_array().expect("commits").len(),
        2,
        "only the two in-window commits: {:?}",
        facts[0]
    );
    assert_eq!(doc["notes"][0]["date"], "2026-08-03");
    assert!(
        doc["gather_failures"]
            .as_array()
            .expect("gather_failures array")
            .is_empty(),
        "no source may fail in a hermetic run: {doc:?}"
    );
    assert_eq!(doc["validation_failure"], serde_json::Value::Null);
}

#[tokio::test]
async fn a_backdated_note_clause_is_scrubbed_end_to_end() {
    let fx = fixture();
    let date = filing_date();

    let run = compile_headless(&fx, date, TriggerSource::Manual).await;

    // The note names FIF-900, whose only commit is outside the window.
    assert_eq!(run.provenance.forbidden_tickets, ["FIF-900"]);
    assert_eq!(run.provenance.range_tickets, ["FIF-201", "FIF-202"]);
    assert_eq!(run.provenance.skew.len(), 1, "{:?}", run.provenance.skew);
    assert_eq!(run.provenance.skew[0].ticket, "FIF-900");
    assert_eq!(run.provenance.skew[0].note_date, d(2026, 8, 3));
    assert_eq!(run.provenance.skew[0].commit_days, [d(2026, 7, 20)]);

    // The raw gathered note still carries the clause; the written file must not.
    let body = read_file(&fx, date);
    assert!(
        !body.contains("FIF-900") && !body.contains("rollout plan"),
        "the backdated clause reached the standup: {body}"
    );
    assert!(
        body.contains("Paired with design on the onboarding walkthrough"),
        "the scrub ate the honest clause too: {body}"
    );
}

#[tokio::test]
async fn an_unchanged_rerun_skips_and_a_new_commit_makes_it_dirty() {
    let fx = fixture();
    let date = filing_date();

    let first = compile_headless(&fx, date, TriggerSource::Manual).await;
    assert_eq!(first.result.status, CompileStatus::Ok);

    // Step (s): only a clean LLM render earns a persisted hash, so a deterministic
    // run leaves the dirty check with nothing to compare against and recompiles.
    assert!(
        !hashes::is_unchanged(&fx.state_dir(), date, HOST, &first.hash),
        "a deterministic render must not persist a hash"
    );
    let second = compile_headless(&fx, date, TriggerSource::Scheduler).await;
    assert_eq!(second.result.status, CompileStatus::Ok);
    assert_eq!(
        second.hash, first.hash,
        "unchanged inputs must hash identically"
    );

    // Stand in for the clean LLM render step (s) requires, then re-run.
    hashes::persist_hash(&fx.state_dir(), date, HOST, &first.hash).expect("persist hash");

    let third = compile_headless(&fx, date, TriggerSource::Scheduler).await;
    assert_eq!(third.result.status, CompileStatus::Skip);
    assert_eq!(third.result.message, "inputs unchanged");

    // A human pressing "Run now" is never silently skipped.
    let manual = compile_headless(&fx, date, TriggerSource::Manual).await;
    assert_eq!(manual.result.status, CompileStatus::Ok);

    // A new in-window commit makes the inputs dirty again. `set_config` clears the
    // gather cache in production; here the fixture changed underneath it instead.
    commit(
        &fx.work_repo(),
        AUTHOR_TWO,
        "2026-08-03 15:00:00",
        "pricing.rs",
        "fix(pricing): FIF-203 clamp the discount ceiling",
    );
    gather::clear_cache(&fx.state_dir()).expect("clear the gather cache");

    let fourth = compile_headless(&fx, date, TriggerSource::Scheduler).await;
    assert_eq!(fourth.result.status, CompileStatus::Ok);
    assert_ne!(
        fourth.hash, first.hash,
        "a new commit must change the inputs hash"
    );
    assert!(read_file(&fx, date).contains("FIF-203"));
}

#[tokio::test]
async fn a_recompile_never_deletes_earlier_work_or_the_manual_region() {
    let mut fx = fixture();
    let date = filing_date();
    seed_existing_file(&fx, date);

    let first = compile_headless(&fx, date, TriggerSource::Manual).await;
    assert_eq!(first.result.status, CompileStatus::Ok);
    assert!(read_file(&fx, date).contains("FIF-201"));

    // Narrow the author filter so FIF-201 drops out of this run's FACTS: the
    // deterministic renderer can no longer produce that bullet, so only
    // `accumulate` can keep it in the file.
    fx.config.standup_authors = vec![AUTHOR_TWO_EMAIL.to_string()];
    gather::clear_cache(&fx.state_dir()).expect("clear the gather cache");

    let second = compile_headless(&fx, date, TriggerSource::Manual).await;
    assert_eq!(second.result.status, CompileStatus::Ok);
    assert!(
        !second.facts.contains("FIF-201"),
        "the narrowed gather should have lost the commit: {}",
        second.facts
    );
    assert!(
        second.result.accumulated_count >= 1,
        "accumulate re-injected nothing"
    );

    let body = read_file(&fx, date);
    assert!(
        body.contains("FIF-201") && body.contains("correct total rounding"),
        "accumulate deleted earlier work: {body}"
    );
    assert!(body.contains("FIF-202"), "{body}");

    let parsed = format::parse(&body).expect("the written file parses");
    assert_eq!(parsed.manual.body.trim(), MANUAL_ITEM);
    assert_eq!(
        parsed
            .auto_for(FOREIGN_HOST)
            .expect("the other host's block survives")
            .body
            .trim(),
        FOREIGN_AUTO
    );
}

#[tokio::test]
async fn the_dailies_repo_gets_a_standup_commit_without_a_coauthor_trailer() {
    let fx = fixture();
    let date = filing_date();

    let run = compile_headless(&fx, date, TriggerSource::Manual).await;
    assert_eq!(run.result.status, CompileStatus::Ok);

    let repo_dir = pipeline_runner::git_root_for(&fx.dailies_dir());
    assert_eq!(
        repo_dir,
        fx.repo_dir(),
        "the dailies dir is a subdirectory of its repo"
    );

    // The sync is warn-only: no remote is configured, and that must not abort.
    git_ops::sync_pull(&repo_dir)
        .await
        .expect("a remote-less pull is a warning, not an error");
    git_ops::ensure_gitattributes(&repo_dir).expect("install the union merge rule");

    let outcome = git_ops::commit_push(&repo_dir, &[fx.file_path(date)])
        .await
        .expect("commit_push");

    assert!(outcome.committed, "the standup file must be committed");
    assert!(
        !outcome.pushed,
        "there is no remote, so the push must fail non-fatally"
    );
    assert_eq!(outcome.message, "standup: 2026-08-04");

    let message = git(&repo_dir, &["log", "-1", "--format=%B"]);
    assert_eq!(message, "standup: 2026-08-04", "no trailers of any kind");
    assert!(
        !message.to_ascii_lowercase().contains("co-authored-by"),
        "the App Script forbids coauthor trailers: {message:?}"
    );

    let listed = git(&repo_dir, &["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(
        listed, "dailies/2026-08-04.md",
        "only the standup file may be staged"
    );
}
