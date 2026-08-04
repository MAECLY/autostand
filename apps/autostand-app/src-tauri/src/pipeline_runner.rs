//! Async pipeline orchestrator: `trigger` → `compile_one` → self-heal → `commit_push`.
//!
//! See `docs/specs/pipeline.md`. This module is the async half of the pipeline:
//! it acquires the run lock, syncs the dailies repo, gathers every enabled data
//! source, asks the configured LLM for a body, and hands the assembled inputs to
//! the pure-sync `autostand_core::pipeline::compile_file`.
//!
//! The split is deliberate: `compile_file` owns scrub → redact → render →
//! accumulate → redact → write → audit and must stay testable without a Tokio
//! runtime, so everything that touches the network, a subprocess, a clock or the
//! Tauri event bus lives here instead.
//!
//! Every *decision* the runner makes — which dates to compile, whether the
//! anti-regression guard fires, whether the LLM body is used or dropped, how the
//! title and subtitle read — is a free function so it can be unit-tested without
//! an `AppHandle`. The `async fn`s are plumbing around those decisions.
//!
//! # Two front ends, one pipeline
//!
//! The `AppHandle` is *optional* throughout. With one ([`trigger`], the IPC
//! path) every step emits its `pipeline-*` event and the config comes out of the
//! Tauri store; without one ([`trigger_headless`], the
//! `autostand-app --compile` path a `launchd`/`systemd`/Task Scheduler unit
//! runs) the events are dropped and the same config is read straight off disk.
//! Nothing else differs — there is one `compile_one`, and a scheduled run with
//! no window open executes exactly the steps the UI would have shown.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Local, NaiveDate};
use tauri::{AppHandle, Emitter};

use autostand_core::dates::{self, Window};
use autostand_core::pipeline::{CompileInputs, CompileOutputs, RenderMode as CoreRenderMode};
use autostand_core::provenance::{self, Provenance};
use autostand_core::{audit, format, hashes, host, redact, scrub};
use autostand_scheduler::lock;
use autostand_scheduler::selfheal;
use autostand_scheduler::triggers::TriggerSource;

use crate::commands::pipeline::{PipelineError, PipelineStarted};
use crate::commands::types::{
    AppConfig, CommitDto, CompileResult, CompileStatus, DateRangeDto, GatherPreview, LastTrigger,
    NoteRef, PipelineProgress, RenderMode, RenderUsed, RepoFacts, SkewRecord,
};
use crate::error::AppError;
use crate::gather::{self, Gathered};
use crate::render::{self, PromptInputs, RenderedBody};
use crate::state::{AppState, PipelineStateKind};

/// Date format used by every `date` argument and DTO field in the IPC contract.
const DATE_FORMAT: &str = "%Y-%m-%d";

/// Lock directory, relative to the state dir (`docs/specs/pipeline.md` § Concurrency).
const LOCK_SUBDIR: &str = "lock";

/// Audit sidecar directory, relative to the state dir. Mirrors the path
/// `autostand_core::audit::write_sidecar` writes to.
const AUDIT_SUBDIR: &str = "audit";

/// Marker the `local-git` source appends to its FACTS carrying the full
/// ticket → commit-day map (`<!-- TICKET_DAYS: FIF-1=2026-08-03,… -->`).
///
/// It must be split back out before the FACTS reach provenance, the LLM prompt
/// or the inputs hash: it names tickets committed *outside* the window, and
/// leaving it in would let every one of them count as an in-window fact.
const TICKET_DAYS_PREFIX: &str = "<!-- TICKET_DAYS:";

/// Section heading the `local-git` source writes per repo.
const REPO_HEADING: &str = "### repo: ";

/// Bundle identifier from `apps/autostand-app/src-tauri/tauri.conf.json`.
///
/// `tauri-plugin-store` resolves a relative store path against
/// `app_data_dir()`, which is `dirs::data_dir()/<identifier>`. The headless
/// entry point has no `AppHandle` to ask, so it reproduces that path.
const APP_IDENTIFIER: &str = "com.miguel50flowers.autostand";

/// Store filename; must stay identical to `commands::CONFIG_STORE_PATH`.
const CONFIG_STORE_FILE: &str = "config.json";

/// Key the `AppConfig` lives under; must stay identical to
/// `commands::CONFIG_STORE_KEY`.
const CONFIG_STORE_KEY: &str = "config";

/// Placeholder the `local-git` source writes when a repo section has no tickets.
const NO_TICKETS: &str = "(none)";

// ── progress steps ────────────────────────────────────────────────────────

/// One observable step of [`compile_one`], with the percent the
/// `pipeline-progress` event reports for it.
///
/// The labels are the `step` strings the frontend shows verbatim and the
/// `pipeline-error` payload reuses, so they are part of the event contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// (a) compute the two-business-day gather window.
    Window,
    /// (b, c, e) run every enabled data source.
    Gather,
    /// (d) anti-regression guard.
    AntiRegression,
    /// (f) `FORBIDDEN` / `COVERED` / `SKEW` provenance.
    Provenance,
    /// (j) hash the inputs and compare with the persisted digest.
    DirtyCheck,
    /// (k) read the existing file's MANUAL region + this host's previous AUTO body.
    ReadExisting,
    /// (m) ask the configured LLM provider for a body.
    RenderLlm,
    /// (n) validate the LLM body against the window's provenance.
    Validate,
    /// (l, o, p, r) deterministic render, accumulate, redact and atomic write.
    Write,
    /// (q) audit sidecar.
    Audit,
    /// (s) persist the inputs hash and report the result.
    Done,
}

/// Every [`Step`] in pipeline order.
///
/// Exposed so a caller (or a test) can enumerate the pipeline without
/// re-deriving the order, and so the strictly-increasing `percent` guarantee has
/// a single list to check.
pub const STEPS: [Step; 11] = [
    Step::Window,
    Step::Gather,
    Step::AntiRegression,
    Step::Provenance,
    Step::DirtyCheck,
    Step::ReadExisting,
    Step::RenderLlm,
    Step::Validate,
    Step::Write,
    Step::Audit,
    Step::Done,
];

impl Step {
    /// Wire label for the `pipeline-progress` / `pipeline-error` `step` field.
    pub fn label(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Gather => "gather",
            Self::AntiRegression => "anti_regression",
            Self::Provenance => "provenance",
            Self::DirtyCheck => "dirty_check",
            Self::ReadExisting => "read_existing",
            Self::RenderLlm => "render_llm",
            Self::Validate => "validate",
            Self::Write => "write",
            Self::Audit => "audit",
            Self::Done => "done",
        }
    }

    /// Progress percent reported before this step runs.
    ///
    /// Strictly increasing, so a client can drive a progress bar from the event
    /// stream alone. `render_llm` is 72 because that is the value the event
    /// example in `docs/tauri/02-ipc-contracts.md` pins.
    pub fn percent(self) -> u8 {
        match self {
            Self::Window => 5,
            Self::Gather => 15,
            Self::AntiRegression => 35,
            Self::Provenance => 45,
            Self::DirtyCheck => 50,
            Self::ReadExisting => 55,
            Self::RenderLlm => 72,
            Self::Validate => 82,
            Self::Write => 90,
            Self::Audit => 96,
            Self::Done => 100,
        }
    }

    /// Coarse `AppState` kind this step belongs to.
    pub fn state_kind(self) -> PipelineStateKind {
        match self {
            Self::Window
            | Self::Gather
            | Self::AntiRegression
            | Self::Provenance
            | Self::DirtyCheck
            | Self::ReadExisting => PipelineStateKind::Gathering,
            Self::RenderLlm | Self::Validate | Self::Write | Self::Audit => {
                PipelineStateKind::Rendering
            }
            Self::Done => PipelineStateKind::Done,
        }
    }
}

// ── run environment ───────────────────────────────────────────────────────

/// Everything a run needs from config + platform, resolved once per [`trigger`].
///
/// Resolved up front so a config write cannot land between two compiles of the
/// same run and silently change the host slug or the output directory halfway.
#[derive(Debug, Clone)]
pub struct RunEnv {
    /// The persisted app config.
    pub config: AppConfig,
    /// Host slug that owns this machine's AUTO block.
    pub host: String,
    /// autostand state dir (lock, hashes, audit sidecars, gather cache).
    pub state_dir: PathBuf,
    /// Directory the `<date>.md` standup files live in.
    pub dailies_dir: PathBuf,
    /// Git working tree that contains [`RunEnv::dailies_dir`].
    pub repo_dir: PathBuf,
}

impl RunEnv {
    /// Resolve the run environment from the persisted config.
    ///
    /// `app` is `None` on the headless path, where the config is read from the
    /// store file directly instead of through the plugin — see
    /// [`load_config_from_disk`].
    pub fn load(app: Option<&AppHandle>) -> Result<Self, AppError> {
        let config = match app {
            Some(handle) => crate::commands::load_config(handle)?,
            None => load_config_from_disk(dirs::data_dir().as_deref())?,
        };
        let state_dir = crate::commands::state_dir();
        let host = resolve_host(&config, &state_dir);
        let dailies_dir = crate::commands::standup::resolve_dailies_dir(
            Some(config.dailies_dir.as_str()),
            dirs::home_dir().as_deref(),
        );
        let repo_dir = git_root_for(&dailies_dir);
        Ok(Self {
            config,
            host,
            state_dir,
            dailies_dir,
            repo_dir,
        })
    }

    /// Path of the standup file for `date`.
    pub fn file_path(&self, date: NaiveDate) -> PathBuf {
        self.dailies_dir.join(format!("{date}.md"))
    }
}

/// Path of the `tauri-plugin-store` file that holds the `AppConfig`.
///
/// `data_dir` is `dirs::data_dir()` — passed in rather than read here so the
/// path rule is testable without depending on the developer's `HOME`.
pub fn config_store_path(data_dir: Option<&Path>) -> Option<PathBuf> {
    data_dir.map(|dir| dir.join(APP_IDENTIFIER).join(CONFIG_STORE_FILE))
}

/// Parse an `AppConfig` out of the raw store file.
///
/// The store serializes a flat `{ key: value }` map, so the config is one entry
/// inside it. A store that exists but has never been written by the Settings
/// page has no `config` key at all, which is not an error — it means defaults.
pub fn config_from_store_bytes(bytes: &[u8]) -> Result<AppConfig, AppError> {
    let store: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(bytes)?;
    match store.get(CONFIG_STORE_KEY) {
        Some(value) => serde_json::from_value(value.clone()).map_err(AppError::from),
        None => Ok(AppConfig::default()),
    }
}

/// Read the persisted `AppConfig` without a Tauri app.
///
/// A missing store file means the app has never saved settings, so defaults are
/// correct. A *corrupt* one is an error: silently compiling with default paths
/// would file standups somewhere the user never configured.
fn load_config_from_disk(data_dir: Option<&Path>) -> Result<AppConfig, AppError> {
    let Some(path) = config_store_path(data_dir) else {
        return Ok(AppConfig::default());
    };
    match std::fs::read(&path) {
        Ok(bytes) => config_from_store_bytes(&bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(err) => Err(AppError::Io(format!("read {}: {err}", path.display()))),
    }
}

/// Resolve the host slug: an explicit, valid override wins over detection.
///
/// A blank or invalid override is ignored rather than honoured — a numeric or
/// IP-like slug forks this machine's AUTO block into a second one, which is the
/// exact failure the host-slug invariant exists to prevent.
pub fn resolve_host(config: &AppConfig, state_dir: &Path) -> String {
    let configured = config
        .host_slug_override
        .as_deref()
        .map(str::trim)
        .filter(|slug| host::is_valid_slug(slug));
    if let Some(slug) = configured {
        return slug.to_string();
    }
    host::load_or_detect(state_dir).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "host slug detection failed");
        "unknown-host".to_string()
    })
}

/// Walk up from `dailies_dir` to the git working tree that contains it.
///
/// `dailies_dir` is often a subdirectory of the dailies repo (`<repo>/dailies`)
/// but is equally often the repo root itself. `git_ops` needs the root: it
/// strips that prefix to build the pathspecs it stages.
pub fn git_root_for(dailies_dir: &Path) -> PathBuf {
    let mut cursor = Some(dailies_dir);
    while let Some(dir) = cursor {
        if crate::git_ops::is_git_repo(dir) {
            return dir.to_path_buf();
        }
        cursor = dir.parent();
    }
    dailies_dir.to_path_buf()
}

// ── pure decisions ────────────────────────────────────────────────────────

/// Filing dates this run must compile, most important first.
///
/// `only_date` is the single-date IPC path (`compile_standup`) and bypasses
/// business-day arithmetic entirely: the caller already named the file.
pub fn targets(today: NaiveDate, only_date: Option<NaiveDate>) -> Vec<NaiveDate> {
    if let Some(date) = only_date {
        return vec![date];
    }
    let (f_today, f_prev) = selfheal::compute_targets(today);
    if f_prev == f_today {
        vec![f_today]
    } else {
        vec![f_today, f_prev]
    }
}

/// Step (d): does the anti-regression guard fire?
///
/// Empty FACTS are only suspicious when the previous run for this `(F, host)`
/// recorded repos: that combination means git or the repo scan failed
/// transiently, and rendering would replace a real standup with "no work".
/// Empty FACTS on a genuinely idle day (no prior sidecar, or one with no repos)
/// are legitimate and must still compile.
pub fn is_regression(facts: &str, previous_repo_count: usize) -> bool {
    facts.trim().is_empty() && previous_repo_count > 0
}

/// How many repos an audit sidecar recorded.
///
/// Reads the enriched `facts` array first; sidecars written before the
/// enrichment step only carry `ticket_days`, so its key count is the fallback
/// signal that the previous run saw commits.
pub fn sidecar_repo_count(sidecar: &serde_json::Value) -> usize {
    if let Some(facts) = sidecar.get("facts").and_then(serde_json::Value::as_array) {
        return facts.len();
    }
    sidecar
        .get("ticket_days")
        .and_then(serde_json::Value::as_object)
        .map_or(0, serde_json::Map::len)
}

/// Step (j): may an unchanged-inputs run skip the render?
///
/// A human pressing "Run now" always gets a fresh compile — a silent skip looks
/// like the button did nothing.
pub fn should_skip_unchanged(source: TriggerSource, unchanged: bool) -> bool {
    unchanged && !source.is_manual()
}

/// Outcome of steps (m) + (n): which body the compile will actually use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderDecision {
    /// `render_mode = Det`: the LLM was never asked.
    Deterministic,
    /// The LLM body passed validation and is used verbatim.
    Accepted(String),
    /// The LLM produced nothing (no provider, disabled, or every attempt failed).
    Missing,
    /// The LLM produced a body that failed validation.
    Rejected {
        /// Stable `ValidationFailure::code` label for the audit sidecar.
        code: &'static str,
        /// Human-readable failure description.
        message: String,
    },
}

impl RenderDecision {
    /// The validated LLM body, when there is one.
    pub fn llm_body(&self) -> Option<&str> {
        if let Self::Accepted(body) = self {
            Some(body)
        } else {
            None
        }
    }

    /// Render mode to hand `autostand_core::pipeline::compile_file`.
    ///
    /// `Llm` for an accepted body — this module already ran step (n), so core
    /// must not second-guess it with its own heuristic — and `Det` for every
    /// fallback, so the deterministic renderer is what writes the file.
    pub fn core_mode(&self) -> CoreRenderMode {
        match self {
            Self::Accepted(_) => CoreRenderMode::Llm,
            _ => CoreRenderMode::Det,
        }
    }

    /// Which renderer the `CompileResult` reports.
    pub fn render_used(&self) -> RenderUsed {
        match self {
            Self::Deterministic => RenderUsed::Det,
            Self::Accepted(_) => RenderUsed::Llm,
            Self::Missing | Self::Rejected { .. } => RenderUsed::LlmFallback,
        }
    }

    /// Whether an LLM render was attempted and lost to the deterministic body.
    pub fn fellback(&self) -> bool {
        matches!(self, Self::Missing | Self::Rejected { .. })
    }

    /// Whether step (s) may persist the inputs hash.
    ///
    /// Only a clean LLM render earns a persisted hash: persisting after a
    /// fallback would make the next run skip a render that never happened.
    pub fn persists_hash(&self) -> bool {
        matches!(self, Self::Accepted(_))
    }
}

/// Steps (m) + (n): decide what the compile renders with.
///
/// Pure. `rendered` is whatever `render::render_llm` returned (`None` when the
/// LLM was unavailable or every attempt failed) and validation is the pure
/// `render::validate_render`. A rejected or missing body is never an error: per
/// `docs/specs/pipeline.md` § Error recovery the compile continues with the
/// deterministic body and reports `fellback = true`. That holds for
/// `render_mode = Llm` too — a configured preference must not be able to leave
/// a working day without a standup.
pub fn decide_render(
    mode: RenderMode,
    rendered: Option<&RenderedBody>,
    range_tickets: &[String],
    forbidden_tickets: &[String],
) -> RenderDecision {
    if mode == RenderMode::Det {
        return RenderDecision::Deterministic;
    }
    let Some(candidate) = rendered else {
        return RenderDecision::Missing;
    };
    match render::validate_render(&candidate.body, range_tickets, forbidden_tickets) {
        Ok(()) => RenderDecision::Accepted(candidate.body.clone()),
        Err(failure) => RenderDecision::Rejected {
            code: failure.code(),
            message: failure.to_string(),
        },
    }
}

/// The `CompileResult.message` for a finished compile.
pub fn outcome_message(decision: &RenderDecision, rendered: Option<&RenderedBody>) -> String {
    match decision {
        RenderDecision::Deterministic => "deterministic render".to_string(),
        RenderDecision::Accepted(_) => match rendered {
            Some(body) => format!("rendered by {} ({})", body.provider, body.model),
            None => "llm render".to_string(),
        },
        RenderDecision::Missing => "llm unavailable; deterministic fallback".to_string(),
        RenderDecision::Rejected { message, .. } => {
            format!("llm render rejected ({message}); deterministic fallback")
        }
    }
}

// ── title + subtitle ──────────────────────────────────────────────────────

/// Standup title for filing date `date` — `docs/specs/standup-file-format.md`
/// § Title format.
///
/// Rendered without the `# Daily Standup — ` prefix, which
/// `autostand_core::format::render` adds.
pub fn title_for(date: NaiveDate) -> String {
    date.format("%B %d, %Y").to_string()
}

/// Standup subtitle for the gather window — `docs/specs/standup-file-format.md`
/// § Subtitle format.
///
/// The range separator is the en-dash `–` (U+2013), never a hyphen. A
/// single-day window drops the range entirely; a same-month range repeats only
/// the day; a cross-year range spells both years out because the trailing
/// `, <year>` would otherwise be ambiguous.
pub fn subtitle_for(window: &Window) -> String {
    let (start, end) = (window.range_start, window.range_end);
    let range = if start == end {
        start.format("%B %d, %Y").to_string()
    } else if start.year() == end.year() {
        if start.month() == end.month() {
            format!(
                "{}–{}, {}",
                start.format("%B %d"),
                end.format("%d"),
                end.format("%Y")
            )
        } else {
            format!(
                "{}–{}, {}",
                start.format("%B %d"),
                end.format("%B %d"),
                end.format("%Y")
            )
        }
    } else {
        format!("{}–{}", start.format("%B %d, %Y"), end.format("%B %d, %Y"))
    };
    format!("_Work completed {range}._")
}

// ── FACTS parsing ─────────────────────────────────────────────────────────

/// Split the `TICKET_DAYS` marker out of a FACTS block.
///
/// Returns the FACTS the pipeline may claim, plus the full ticket → commit-day
/// map `autostand_core::provenance::compute` needs to tell "committed on
/// another day" apart from "no git footprint at all".
pub fn split_ticket_days(facts: &str) -> (String, HashMap<String, Vec<NaiveDate>>) {
    let mut kept: Vec<&str> = Vec::new();
    let mut days: HashMap<String, Vec<NaiveDate>> = HashMap::new();
    for line in facts.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(TICKET_DAYS_PREFIX) {
            merge_ticket_days(&mut days, rest.strip_suffix("-->").unwrap_or(rest).trim());
        } else {
            kept.push(line);
        }
    }
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    for values in days.values_mut() {
        values.sort_unstable();
    }
    let mut clean = kept.join("\n");
    if !clean.is_empty() {
        clean.push('\n');
    }
    (clean, days)
}

/// Parse one `KEY=day,day,KEY2=day` payload into `days`.
///
/// The payload is flat comma-separated text, so a token carrying `=` opens a new
/// ticket and every following bare token is another day for it.
fn merge_ticket_days(days: &mut HashMap<String, Vec<NaiveDate>>, payload: &str) {
    let mut current: Option<String> = None;
    for raw in payload.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let day_text = if let Some((key, first)) = token.split_once('=') {
            current = Some(key.trim().to_string());
            first.trim()
        } else {
            token
        };
        let Some(key) = current.as_ref() else {
            continue;
        };
        let Ok(day) = NaiveDate::parse_from_str(day_text, DATE_FORMAT) else {
            continue;
        };
        let entry = days.entry(key.clone()).or_default();
        if !entry.contains(&day) {
            entry.push(day);
        }
    }
}

/// Parse a FACTS block into the `RepoFacts` DTO the debug preview and the audit
/// sidecar carry.
///
/// The FACTS block is text — that is what the LLM prompt and the deterministic
/// renderer consume — so the structured view is reconstructed here rather than
/// threaded through every data source.
pub fn parse_repo_facts(facts: &str) -> Vec<RepoFacts> {
    let mut out: Vec<RepoFacts> = Vec::new();
    for line in facts.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(REPO_HEADING) {
            out.push(parse_repo_heading(rest));
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("- ") else {
            continue;
        };
        let Some(section) = out.last_mut() else {
            continue;
        };
        let (ticket, subject) = split_fact_bullet(rest);
        if section.title.is_empty() {
            section.title.clone_from(&subject);
        }
        if section.ticket.is_none() {
            section.ticket = ticket;
        }
        section.commits.push(CommitDto {
            sha: String::new(),
            subject,
            date: String::new(),
            files: Vec::new(),
        });
    }
    out
}

/// Parse `<repo> / tickets: <keys> / commits (N):` into an empty `RepoFacts`.
fn parse_repo_heading(rest: &str) -> RepoFacts {
    let mut parts = rest.split(" / ");
    let repo = parts.next().unwrap_or(rest).trim().to_string();
    let ticket = parts
        .next()
        .and_then(|part| part.trim().strip_prefix("tickets: "))
        .map(str::trim)
        .filter(|keys| !keys.is_empty() && *keys != NO_TICKETS)
        .and_then(|keys| keys.split(',').next())
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty());
    RepoFacts {
        repo,
        ticket,
        title: String::new(),
        commits: Vec::new(),
    }
}

/// Split a `(KEY) subject` FACTS bullet into its ticket and its subject.
///
/// `local-git` writes `-` for a commit whose subject carries no ticket key.
fn split_fact_bullet(rest: &str) -> (Option<String>, String) {
    let Some(after) = rest.strip_prefix('(') else {
        return (None, rest.trim().to_string());
    };
    let Some(close) = after.find(')') else {
        return (None, rest.trim().to_string());
    };
    let key = after[..close].trim();
    let subject = after[close + 1..].trim().to_string();
    let ticket = if key.is_empty() || key == "-" {
        None
    } else {
        Some(key.to_string())
    };
    (ticket, subject)
}

/// Parse the gathered notes blob into the `NoteRef` DTO.
///
/// The blob is `## <ISO date>` sections of bullet lines. `source` stays empty:
/// the blob no longer carries the file each section came from, and the preview
/// shows the date instead.
pub fn parse_note_refs(notes: &str) -> Vec<NoteRef> {
    let mut out: Vec<NoteRef> = Vec::new();
    for line in notes.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("## ") {
            out.push(NoteRef {
                source: String::new(),
                date: rest.trim().to_string(),
                clauses: Vec::new(),
            });
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let clause = trimmed.strip_prefix("- ").unwrap_or(trimmed).trim();
        if clause.is_empty() {
            continue;
        }
        if out.is_empty() {
            out.push(NoteRef::default());
        }
        if let Some(note) = out.last_mut() {
            note.clauses.push(clause.to_string());
        }
    }
    out
}

/// Hash the gathered inputs for step (j).
///
/// Covers every source listed in `docs/architecture/03-data-flow.md` step 5h, so
/// a change in any of them re-renders.
pub fn inputs_hash(facts: &str, gathered: &Gathered) -> String {
    let claude = gathered.claude_files.join("\n");
    let opencode = gathered.opencode_sessions.join("\n");
    let codex = gathered.codex_sessions.join("\n");
    let gemini = gathered.gemini_sessions.join("\n");
    let grok = gathered.grok_sessions.join("\n");
    hashes::input_hash(&[
        facts,
        gathered.notes.as_str(),
        gathered.conv.as_deref().unwrap_or_default(),
        gathered.prrev.as_deref().unwrap_or_default(),
        gathered.github.as_deref().unwrap_or_default(),
        claude.as_str(),
        opencode.as_str(),
        codex.as_str(),
        gemini.as_str(),
        grok.as_str(),
    ])
}

// ── result construction ───────────────────────────────────────────────────

/// `CompileResult` skeleton: identity + file path, everything else defaulted.
pub fn base_result(date: NaiveDate, host: &str, file_path: &Path) -> CompileResult {
    CompileResult {
        date: date.format(DATE_FORMAT).to_string(),
        host: host.to_string(),
        status: CompileStatus::Ok,
        render_used: RenderUsed::Det,
        fellback: false,
        audit_path: None,
        file_path: file_path.display().to_string(),
        accumulated_count: 0,
        message: String::new(),
    }
}

/// `CompileResult` for a compile that wrote nothing (`status: "skip"`).
pub fn skip_result(
    date: NaiveDate,
    host: &str,
    file_path: &Path,
    message: impl Into<String>,
) -> CompileResult {
    CompileResult {
        status: CompileStatus::Skip,
        message: message.into(),
        ..base_result(date, host, file_path)
    }
}

/// `CompileResult` for a compile that failed (`status: "error"`).
///
/// `file_path` stays empty: an error means nothing was written, and pointing at
/// a path that may not exist would make the UI offer to open a missing file.
pub fn error_result(date: NaiveDate, host: &str, message: impl Into<String>) -> CompileResult {
    CompileResult {
        status: CompileStatus::Error,
        message: message.into(),
        ..base_result(date, host, Path::new(""))
    }
}

/// `CompileResult` for a compile that wrote the file (`status: "ok"`).
///
/// `status` stays `ok` even when the LLM lost to the deterministic renderer:
/// only a write failure is an error (`docs/specs/pipeline.md` § Error recovery).
pub fn ok_result(
    date: NaiveDate,
    host: &str,
    file_path: &Path,
    decision: &RenderDecision,
    outputs: &CompileOutputs,
    message: impl Into<String>,
) -> CompileResult {
    CompileResult {
        render_used: decision.render_used(),
        fellback: decision.fellback(),
        audit_path: outputs
            .audit_path
            .as_ref()
            .map(|path| path.display().to_string()),
        accumulated_count: outputs.accumulated_count,
        message: message.into(),
        ..base_result(date, host, file_path)
    }
}

// ── audit sidecar enrichment ──────────────────────────────────────────────

/// The sidecar fields only the async layer knows, as a JSON patch.
///
/// `autostand_core::pipeline::compile_file` writes the sidecar from the pure
/// inputs it was handed, so it cannot fill in the gathered source blocks, the
/// real gather window, the provider that rendered, or the fact that step (n)
/// rejected the body. Those are exactly the fields `docs/specs/audit.md`
/// specifies, so they are patched over the sidecar core just wrote.
///
/// `window` keeps core's `start` / `end` key names — `commands::standup` maps
/// them onto the contract's `range_start` / `range_end` and would otherwise read
/// an empty range. Getting the window right matters beyond cosmetics:
/// `autostand_core::audit::classify` tests each commit day against it, and
/// core's placeholder (`F..F`) classifies every real bullet as `unverified`.
pub fn audit_patch(
    window: &Window,
    facts: &str,
    gathered: &Gathered,
    decision: &RenderDecision,
    rendered: Option<&RenderedBody>,
    hash: &str,
) -> serde_json::Value {
    let render_used = match decision.render_used() {
        RenderUsed::Llm => "llm",
        RenderUsed::Det => "det",
        RenderUsed::LlmFallback => "llm_fallback",
    };
    let validation = match decision {
        RenderDecision::Rejected { code, message } => {
            serde_json::json!({ "code": code, "message": message })
        }
        _ => serde_json::Value::Null,
    };
    let failures: Vec<serde_json::Value> = gathered
        .failures
        .iter()
        .map(|(source, error)| serde_json::json!({ "source": source, "error": error }))
        .collect();
    serde_json::json!({
        "window": { "start": window.range_start, "end": window.range_end },
        "facts": parse_repo_facts(facts),
        "notes": parse_note_refs(&gathered.notes),
        "github": gathered.github,
        "conv": gathered.conv,
        "prrev": gathered.prrev,
        "claude_files": gathered.claude_files,
        "opencode_sessions": gathered.opencode_sessions,
        "codex_sessions": gathered.codex_sessions,
        "gemini_sessions": gathered.gemini_sessions,
        "grok_sessions": gathered.grok_sessions,
        "render_used": render_used,
        "fellback": decision.fellback(),
        "provider": rendered.map(|body| body.provider.clone()),
        "model": rendered.map(|body| body.model.clone()),
        "hash": hash,
        "gather_failures": failures,
        "validation_failure": validation,
    })
}

/// Merge `overlay`'s top-level keys into the JSON object at `path`, atomically.
fn enrich_sidecar(path: &Path, overlay: &serde_json::Value) -> Result<(), AppError> {
    let bytes = std::fs::read(path)?;
    let mut doc: serde_json::Value = serde_json::from_slice(&bytes)?;
    {
        let Some(target) = doc.as_object_mut() else {
            return Err(AppError::Config("audit sidecar is not an object".into()));
        };
        let Some(source) = overlay.as_object() else {
            return Err(AppError::Config("audit patch is not an object".into()));
        };
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    write_json_atomic(path, &serde_json::to_string_pretty(&doc)?)
}

/// Write `text` to `path` atomically (temp → fsync → rename), mode `0600`.
fn write_json_atomic(path: &Path, text: &str) -> Result<(), AppError> {
    use std::io::Write as _;

    let tmp = path.with_extension("json.tmp");
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    set_mode_0600(path)
}

/// Tighten `path` to owner-only read/write; the sidecar carries redacted but
/// still private conversation digests.
#[cfg(unix)]
fn set_mode_0600(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// Windows has no POSIX mode bits; the state dir itself is user-scoped.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn set_mode_0600(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

// ── disk helpers ──────────────────────────────────────────────────────────

/// Path of the audit sidecar for `(date, host)`.
fn sidecar_path(state_dir: &Path, date: NaiveDate, host: &str) -> PathBuf {
    state_dir
        .join(AUDIT_SUBDIR)
        .join(format!("{date}-{host}.json"))
}

/// How many repos the previous run recorded for `(date, host)`; 0 when there is
/// no readable sidecar.
fn previous_repo_count(state_dir: &Path, date: NaiveDate, host: &str) -> usize {
    let path = sidecar_path(state_dir, date, host);
    let Ok(bytes) = std::fs::read(&path) else {
        return 0;
    };
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .as_ref()
        .map_or(0, sidecar_repo_count)
}

/// Step (k): this host's existing AUTO body for `date`, if the file has one.
///
/// `None` means "no file, or no block for this host" — which is what
/// `autostand_scheduler::selfheal::is_frozen` distinguishes from an empty block.
fn read_auto_body(dailies_dir: &Path, date: NaiveDate, host: &str) -> Option<String> {
    let content = std::fs::read_to_string(dailies_dir.join(format!("{date}.md"))).ok()?;
    let parsed = format::parse(&content).ok()?;
    parsed
        .auto_blocks
        .into_iter()
        .find(|block| block.host == host)
        .map(|block| block.body)
}

// ── DTO conversions ───────────────────────────────────────────────────────

/// The gather window as the contract's `{ range_start, range_end }` DTO.
fn range_dto(window: &Window) -> DateRangeDto {
    DateRangeDto {
        range_start: window.range_start.format(DATE_FORMAT).to_string(),
        range_end: window.range_end.format(DATE_FORMAT).to_string(),
    }
}

/// Core `SkewRecord`s as the contract's string-dated DTO.
fn skew_dtos(skew: &[audit::SkewRecord]) -> Vec<SkewRecord> {
    skew.iter()
        .map(|record| SkewRecord {
            ticket: record.ticket.clone(),
            note_date: record.note_date.format(DATE_FORMAT).to_string(),
            commit_days: record
                .commit_days
                .iter()
                .map(|day| day.format(DATE_FORMAT).to_string())
                .collect(),
        })
        .collect()
}

/// Map a `TriggerSource` onto the three-bucket IPC `LastTrigger`.
///
/// Routed through `TriggerSource::wire_label` so the two vocabularies cannot
/// drift apart.
pub fn last_trigger(source: TriggerSource) -> LastTrigger {
    match source.wire_label() {
        "manual" => LastTrigger::Manual,
        "self-heal" => LastTrigger::SelfHeal,
        _ => LastTrigger::Scheduled,
    }
}

// ── event emission ────────────────────────────────────────────────────────

/// Emit a Tauri event, downgrading a failed emit to a warning.
///
/// A window that has gone away must never abort a compile that is otherwise
/// succeeding: the file on disk is the deliverable, the event is a courtesy.
/// `None` is the headless sink — a `launchd`/`systemd`/Task Scheduler run has
/// no window to notify, so every event is dropped and nothing else changes.
fn emit<P: serde::Serialize + Clone>(app: Option<&AppHandle>, event: &str, payload: P) {
    let Some(app) = app else {
        return;
    };
    if let Err(err) = app.emit(event, payload) {
        tracing::warn!(event, error = %err, "failed to emit pipeline event");
    }
}

/// Emit `pipeline-progress` for `step` and keep `AppState` in sync so
/// `get_pipeline_status` reflects the same position as the event stream.
///
/// `AppState` is updated even headlessly: it costs nothing, and it keeps the
/// two front ends walking literally the same code.
fn progress(app: Option<&AppHandle>, state: &AppState, date: &str, host: &str, step: Step) {
    state.set_percent(step.percent(), step.label().to_string());
    emit(
        app,
        "pipeline-progress",
        PipelineProgress {
            date: date.to_string(),
            host: host.to_string(),
            step: step.label().to_string(),
            percent: step.percent(),
        },
    );
}

/// Emit `pipeline-error` without touching `AppState`.
///
/// Used for the non-fatal failures the spec still surfaces to the UI — an LLM
/// fallback and the anti-regression skip — where the run is not in an error
/// state and `CompileResult.status` stays `ok` / `skip`.
fn warn_event(app: Option<&AppHandle>, date: &str, step: Step, code: &str, message: &str) {
    emit(
        app,
        "pipeline-error",
        PipelineError {
            code: code.to_string(),
            message: message.to_string(),
            step: step.label().to_string(),
            date: date.to_string(),
        },
    );
}

/// Emit `pipeline-error` *and* put `AppState` into the error state.
fn fail(
    app: Option<&AppHandle>,
    state: &AppState,
    date: &str,
    step: Step,
    code: &str,
    message: &str,
) {
    state.set_error(message.to_string());
    warn_event(app, date, step, code, message);
}

// ── the pipeline ──────────────────────────────────────────────────────────

/// Steps 1–5 of `docs/specs/pipeline.md`: lock, sync, compute targets, compile
/// each, self-heal `F_PREV`, commit + push, emit `pipeline-done` per target.
///
/// The run lock is held by an RAII guard for the whole body, so it is released
/// even if a step panics or the future is cancelled mid-`await`. A lock held by
/// a live process aborts immediately with `AppError::Lock` and a
/// `pipeline-error` carrying `code: "lock"` — the scheduler does not queue.
///
/// Never returns `Err` for a *compile* failure: a failed date lands in the
/// returned vector as `status: "error"` so a second target still gets its turn.
pub async fn trigger(
    app: &AppHandle,
    state: &AppState,
    source: TriggerSource,
    only_date: Option<NaiveDate>,
) -> Result<Vec<CompileResult>, AppError> {
    run(Some(app), state, source, only_date).await
}

/// [`trigger`] with no window: the `autostand-app --compile` entry point.
///
/// Identical pipeline, identical lock, identical last-run stamp; the only
/// difference is that the `pipeline-*` events go nowhere and the config is read
/// off disk. This is what a `launchd` agent, a `systemd --user` timer or a
/// Task Scheduler task actually runs.
pub async fn trigger_headless(
    state: &AppState,
    source: TriggerSource,
    only_date: Option<NaiveDate>,
) -> Result<Vec<CompileResult>, AppError> {
    run(None, state, source, only_date).await
}

/// The run itself, shared by both front ends.
#[tracing::instrument(skip_all, fields(trigger = source.label(), only_date = ?only_date, headless = app.is_none()))]
async fn run(
    app: Option<&AppHandle>,
    state: &AppState,
    source: TriggerSource,
    only_date: Option<NaiveDate>,
) -> Result<Vec<CompileResult>, AppError> {
    let env = RunEnv::load(app)?;
    let today = Local::now().date_naive();
    let dates = targets(today, only_date);
    let primary = dates.first().copied().unwrap_or(today);
    let primary_str = primary.format(DATE_FORMAT).to_string();

    // 1. Lock. Fails fast; the guard releases on every exit path.
    let _guard = lock::acquire(&env.state_dir.join(LOCK_SUBDIR)).map_err(|err| {
        let message = format!("another compile is already running ({err})");
        fail(app, state, &primary_str, Step::Window, "lock", &message);
        AppError::Lock(message)
    })?;

    // 2. Announce, then sync the dailies repo (warn-only: the local copy wins).
    emit(
        app,
        "pipeline-started",
        PipelineStarted {
            date: primary_str,
            host: env.host.clone(),
            trigger: last_trigger(source),
        },
    );
    if let Err(err) = crate::git_ops::sync_pull(&env.repo_dir).await {
        tracing::warn!(error = %err, "dailies sync skipped; continuing with the local copy");
    }
    if let Err(err) = crate::git_ops::ensure_gitattributes(&env.repo_dir) {
        tracing::warn!(error = %err, "could not install the union merge driver rule");
    }

    // 3-5. Compile each target; the second one is the self-heal slot.
    let mut results: Vec<CompileResult> = Vec::with_capacity(dates.len());
    let mut touched: Vec<PathBuf> = Vec::with_capacity(dates.len());
    for (index, date) in dates.iter().copied().enumerate() {
        let (target_source, blocked) = if index == 0 {
            (source, None)
        } else {
            self_heal_gate(&env, date, source)
        };
        if let Some(reason) = blocked {
            tracing::info!(date = %date, reason, "self-heal skipped");
            results.push(skip_result(date, &env.host, &env.file_path(date), reason));
            continue;
        }
        let result = compile_one(app, state, &env, date, target_source)
            .await
            .unwrap_or_else(|err| {
                let message = err.to_string();
                tracing::error!(date = %date, error = %message, "compile failed");
                let date_str = date.format(DATE_FORMAT).to_string();
                fail(app, state, &date_str, Step::Write, "io", &message);
                error_result(date, &env.host, message)
            });
        if result.status == CompileStatus::Ok {
            touched.push(env.file_path(date));
        }
        results.push(result);
    }

    // 6. Commit + push (warn-only), then announce every result.
    match crate::git_ops::commit_push(&env.repo_dir, &touched).await {
        Ok(outcome) => tracing::info!(
            committed = outcome.committed,
            pushed = outcome.pushed,
            message = %outcome.message,
            "commit_push finished"
        ),
        Err(err) => tracing::warn!(error = %err, "commit_push skipped"),
    }

    // `SchedulerStatus.last_run_at` means "when did this machine last compile",
    // not "when did cron last fire", so a manual or self-heal run stamps it too.
    // Reached only past the lock, so a refused run never claims one.
    crate::scheduler_runtime::record_run(last_trigger(source));

    for result in &results {
        emit(app, "pipeline-done", result.clone());
    }
    Ok(results)
}

/// Step 5: should `F_PREV` be self-healed, and if not, why not?
///
/// Returns the trigger source to compile it under, plus `Some(reason)` when the
/// day must be left alone. A populated AUTO block is **frozen**: self-heal runs
/// from durable disk data only, so recompiling it would risk rewriting a
/// complete standup from a strictly narrower input set.
fn self_heal_gate(
    env: &RunEnv,
    date: NaiveDate,
    source: TriggerSource,
) -> (TriggerSource, Option<&'static str>) {
    if !env.config.scheduler.self_heal {
        return (source, Some("self-heal disabled"));
    }
    let existing = read_auto_body(&env.dailies_dir, date, &env.host);
    if selfheal::is_frozen(existing.as_deref()) {
        return (source, Some("frozen"));
    }
    (TriggerSource::SelfHeal, None)
}

/// Step 3 (a…s) of `docs/specs/pipeline.md`: compile one filing date.
///
/// Returns `Err` only when the file could not be written — every other failure
/// (a dead data source, an absent LLM, a rejected render) degrades to the
/// deterministic body and still produces a `status: "ok"` result.
// The step list *is* the function: splitting it would hide the ordering the
// spec pins. Each step's decision already lives in its own tested free function.
#[allow(clippy::too_many_lines)]
#[tracing::instrument(skip_all, fields(date = %date, host = %env.host, trigger = source.label()))]
pub async fn compile_one(
    app: Option<&AppHandle>,
    state: &AppState,
    env: &RunEnv,
    date: NaiveDate,
    source: TriggerSource,
) -> Result<CompileResult, AppError> {
    let date_str = date.format(DATE_FORMAT).to_string();
    let file_path = env.file_path(date);
    state.set_state(
        date_str.clone(),
        env.host.clone(),
        Step::Window.state_kind(),
        Step::Window.label().to_string(),
    );

    // (a) window
    progress(app, state, &date_str, &env.host, Step::Window);
    let window = dates::compute_window(date);

    // (b, c, e) gather — never fatal; failures are recorded on `Gathered`.
    progress(app, state, &date_str, &env.host, Step::Gather);
    let gathered = gather::gather_all(
        &gather::to_date_window(&window),
        &env.config,
        &env.state_dir,
    )
    .await;
    let (facts, all_git_tickets) = split_ticket_days(&gathered.facts);

    // (d) anti-regression guard
    progress(app, state, &date_str, &env.host, Step::AntiRegression);
    let previous_repos = previous_repo_count(&env.state_dir, date, &env.host);
    if is_regression(&facts, previous_repos) {
        let message =
            format!("FACTS empty but the last run recorded {previous_repos} repos; not writing");
        tracing::warn!(date = %date, "{message}");
        warn_event(
            app,
            &date_str,
            Step::AntiRegression,
            "anti_regression",
            &message,
        );
        let result = skip_result(date, &env.host, &file_path, message);
        state.set_done(result.clone());
        return Ok(result);
    }

    // (f) provenance
    progress(app, state, &date_str, &env.host, Step::Provenance);
    let prov = provenance::compute(
        &facts,
        &gathered.notes,
        gathered.github.as_deref(),
        &all_git_tickets,
        &window.dates,
    );

    // (j) dirty check
    progress(app, state, &date_str, &env.host, Step::DirtyCheck);
    let hash = inputs_hash(&facts, &gathered);
    if should_skip_unchanged(
        source,
        hashes::is_unchanged(&env.state_dir, date, &env.host, &hash),
    ) {
        tracing::info!(date = %date, "inputs unchanged since the last clean render");
        let result = skip_result(date, &env.host, &file_path, "inputs unchanged");
        state.set_done(result.clone());
        return Ok(result);
    }

    // (k) read existing
    progress(app, state, &date_str, &env.host, Step::ReadExisting);
    let prev_auto = read_auto_body(&env.dailies_dir, date, &env.host).unwrap_or_default();

    // (m) LLM render. `render_body` short-circuits for `Det` and never errors.
    state.set_state(
        date_str.clone(),
        env.host.clone(),
        Step::RenderLlm.state_kind(),
        Step::RenderLlm.label().to_string(),
    );
    progress(app, state, &date_str, &env.host, Step::RenderLlm);
    let rendered = render_body(env, &facts, &gathered, &prov, &window, date, &prev_auto).await;

    // (n) validate
    progress(app, state, &date_str, &env.host, Step::Validate);
    let decision = decide_render(
        env.config.render_mode,
        rendered.as_ref(),
        &prov.range_tickets,
        &prov.forbidden_tickets,
    );
    if let RenderDecision::Rejected { code, message } = &decision {
        tracing::warn!(code, "{message}");
        warn_event(app, &date_str, Step::RenderLlm, "llm", message);
    } else if decision == RenderDecision::Missing {
        let message = "no LLM body produced; using the deterministic render";
        tracing::warn!("{message}");
        warn_event(app, &date_str, Step::RenderLlm, "llm", message);
    }

    // (l, o, p, r) deterministic render, accumulate, redact, atomic write.
    progress(app, state, &date_str, &env.host, Step::Write);
    std::fs::create_dir_all(&env.dailies_dir)?;
    let inputs = CompileInputs {
        facts: facts.clone(),
        notes: gathered.notes.clone(),
        github: gathered.github.clone(),
        conv: gathered.conv.clone(),
        prrev: gathered.prrev.clone(),
        claude_files: gathered.claude_files.clone(),
        prev_auto_body: prev_auto,
        forbidden_tickets: prov.forbidden_tickets.clone(),
        covered_tickets: prov.covered_tickets.clone(),
        ticket_days: prov.ticket_days.clone(),
        skew: prov.skew.clone(),
        jira_base: env.config.jira_base.clone(),
        render_mode: decision.core_mode(),
        llm_body: decision.llm_body().map(str::to_string),
        host: env.host.clone(),
        state_dir: env.state_dir.clone(),
        dailies_dir: env.dailies_dir.clone(),
        file_date: date,
        title: title_for(date),
        subtitle: subtitle_for(&window),
    };
    let outputs = autostand_core::pipeline::compile_file(&inputs)
        .map_err(|e| AppError::Io(format!("write {}: {e}", file_path.display())))?;

    // (q) audit sidecar: patch in what the pure core could not know.
    progress(app, state, &date_str, &env.host, Step::Audit);
    if let Some(path) = outputs.audit_path.as_ref() {
        let overlay = audit_patch(
            &window,
            &facts,
            &gathered,
            &decision,
            rendered.as_ref(),
            &hash,
        );
        if let Err(err) = enrich_sidecar(path, &overlay) {
            tracing::warn!(error = %err, "could not enrich the audit sidecar");
        }
    }

    // (s) persist the inputs hash only after a clean LLM render.
    progress(app, state, &date_str, &env.host, Step::Done);
    if decision.persists_hash() {
        if let Err(err) = hashes::persist_hash(&env.state_dir, date, &env.host, &hash) {
            tracing::warn!(error = %err, "could not persist the inputs hash");
        }
    }

    let result = ok_result(
        date,
        &env.host,
        &file_path,
        &decision,
        &outputs,
        outcome_message(&decision, rendered.as_ref()),
    );
    tracing::info!(
        render_used = ?result.render_used,
        accumulated = result.accumulated_count,
        gather_failures = gathered.failures.len(),
        "compile finished"
    );
    state.set_done(result.clone());
    Ok(result)
}

/// Step (m): build the render prompt from the gathered inputs and call the LLM.
///
/// The prompt gets *scrubbed and redacted* notes: the LLM must never see a
/// clause the anti-backdating rules already dropped, nor a secret the final
/// redaction pass would only have removed after the fact.
async fn render_body(
    env: &RunEnv,
    facts: &str,
    gathered: &Gathered,
    prov: &Provenance,
    window: &Window,
    date: NaiveDate,
    prev_auto: &str,
) -> Option<RenderedBody> {
    if env.config.render_mode == RenderMode::Det {
        return None;
    }
    let meta_extra = env.config.scrub.meta_extra.as_deref();
    let facts_clean = redact::redact(facts);
    let notes_clean = redact::redact(&scrub::scrub_notes(
        &gathered.notes,
        &prov.forbidden_tickets,
        &prov.covered_tickets,
        meta_extra,
    ));
    let github = gathered.github.as_deref().map(redact::redact);
    let conv = gathered
        .conv
        .as_deref()
        .map(|text| {
            scrub::scrub_notes(
                text,
                &prov.forbidden_tickets,
                &prov.covered_tickets,
                meta_extra,
            )
        })
        .map(|text| redact::redact(&text));
    let prrev = gathered.prrev.as_deref().map(redact::redact);

    let file_date = date.format(DATE_FORMAT).to_string();
    let range_start = window.range_start.format(DATE_FORMAT).to_string();
    let range_end = window.range_end.format(DATE_FORMAT).to_string();
    let title = title_for(date);
    let subtitle = subtitle_for(window);

    let inputs = PromptInputs {
        facts: &facts_clean,
        github: github.as_deref(),
        notes: &notes_clean,
        conv: conv.as_deref(),
        prrev: prrev.as_deref(),
        jira_base: &env.config.jira_base,
        file_date: &file_date,
        range_start: &range_start,
        range_end: &range_end,
        title: &title,
        subtitle: &subtitle,
        prev_auto: Some(prev_auto).filter(|body| !body.trim().is_empty()),
    };
    render::render_llm(&inputs, &env.config).await
}

/// Gather-only debug path (`preview_gather`): steps (a), (b), (c), (e) and (f).
///
/// Takes no lock and writes nothing — it is a read-only view of what a compile
/// of `date` would see right now, which is exactly what the Debug page wants.
#[tracing::instrument(skip_all, fields(date = %date))]
pub async fn preview(app: &AppHandle, date: NaiveDate) -> Result<GatherPreview, AppError> {
    let env = RunEnv::load(Some(app))?;
    let window = dates::compute_window(date);
    let gathered = gather::gather_all(
        &gather::to_date_window(&window),
        &env.config,
        &env.state_dir,
    )
    .await;
    let (facts, all_git_tickets) = split_ticket_days(&gathered.facts);
    let prov = provenance::compute(
        &facts,
        &gathered.notes,
        gathered.github.as_deref(),
        &all_git_tickets,
        &window.dates,
    );
    Ok(GatherPreview {
        date: date.format(DATE_FORMAT).to_string(),
        host: env.host,
        window: range_dto(&window),
        facts: parse_repo_facts(&facts),
        notes: parse_note_refs(&gathered.notes),
        github: gathered.github,
        conv: gathered.conv,
        prrev: gathered.prrev,
        claude_files: gathered.claude_files,
        opencode_sessions: gathered.opencode_sessions,
        codex_sessions: gathered.codex_sessions,
        gemini_sessions: gathered.gemini_sessions,
        grok_sessions: gathered.grok_sessions,
        forbidden_tickets: prov.forbidden_tickets,
        covered_tickets: prov.covered_tickets,
        skew: skew_dtos(&prov.skew),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        audit_patch, base_result, config_from_store_bytes, config_store_path, decide_render,
        error_result, git_root_for, inputs_hash, is_regression, last_trigger,
        load_config_from_disk, ok_result, outcome_message, parse_note_refs, parse_repo_facts,
        should_skip_unchanged, sidecar_repo_count, skip_result, split_ticket_days, subtitle_for,
        targets, title_for, RenderDecision, Step, STEPS,
    };
    use crate::commands::types::{
        AppConfig, CompileStatus, LastTrigger, RenderMode, RenderUsed, SchedulerConfig,
    };
    use crate::error::AppError;
    use crate::gather::Gathered;
    use crate::render::RenderedBody;
    use crate::state::PipelineStateKind;
    use autostand_core::dates::{compute_window, Window};
    use autostand_core::pipeline::{CompileOutputs, CoreRenderModeUsed, RenderMode as CoreMode};
    use autostand_scheduler::triggers::TriggerSource;
    use chrono::NaiveDate;
    use std::path::{Path, PathBuf};

    /// Build a date, panicking on an invalid one (test-only convenience).
    fn d(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
    }

    /// A `RenderedBody` as `render::render_llm` would return it.
    fn llm(body: &str) -> RenderedBody {
        RenderedBody {
            body: body.to_string(),
            provider: "claude".to_string(),
            model: "sonnet".to_string(),
            used_api: false,
        }
    }

    /// A window with an explicit range (bypassing business-day arithmetic).
    fn window(start: NaiveDate, end: NaiveDate) -> Window {
        Window {
            range_start: start,
            range_end: end,
            dates: vec![start, end],
        }
    }

    // ── target selection ──────────────────────────────────────────────────

    #[test]
    fn only_date_compiles_exactly_that_date() {
        assert_eq!(
            targets(d(2026, 8, 3), Some(d(2026, 1, 2))),
            vec![d(2026, 1, 2)]
        );
    }

    #[test]
    fn only_date_wins_even_for_a_weekend_date() {
        // The IPC caller named the file; business-day arithmetic must not move it.
        let saturday = d(2026, 8, 8);
        assert_eq!(targets(d(2026, 8, 3), Some(saturday)), vec![saturday]);
    }

    #[test]
    fn without_only_date_the_targets_are_f_today_then_f_prev() {
        // Mon 2026-08-03 → file today's work on Tue, previous target is Mon.
        assert_eq!(
            targets(d(2026, 8, 3), None),
            vec![d(2026, 8, 4), d(2026, 8, 3)]
        );
    }

    #[test]
    fn weekend_targets_collapse_onto_monday_and_friday() {
        for weekend_day in [d(2026, 8, 8), d(2026, 8, 9)] {
            assert_eq!(
                targets(weekend_day, None),
                vec![d(2026, 8, 10), d(2026, 8, 7)],
                "{weekend_day} must file on Monday with Friday as the previous target"
            );
        }
    }

    #[test]
    fn targets_are_always_two_distinct_days() {
        let mut day = d(2026, 1, 1);
        while day < d(2027, 1, 1) {
            let selected = targets(day, None);
            assert_eq!(selected.len(), 2, "{day}");
            assert!(selected[1] < selected[0], "{day}: {selected:?}");
            day = day.succ_opt().expect("next day");
        }
    }

    // ── anti-regression ───────────────────────────────────────────────────

    #[test]
    fn empty_facts_after_a_run_with_repos_is_a_regression() {
        assert!(is_regression("", 3));
        assert!(is_regression("   \n\n\t", 1));
    }

    #[test]
    fn empty_facts_on_a_first_run_is_not_a_regression() {
        // No prior sidecar ⇒ nothing to regress from; an idle day must compile.
        assert!(!is_regression("", 0));
    }

    #[test]
    fn non_empty_facts_are_never_a_regression() {
        assert!(!is_regression("### repo: autostand / tickets: FIF-1", 9));
    }

    #[test]
    fn repo_count_prefers_the_facts_array() {
        let sidecar = serde_json::json!({
            "facts": [{ "repo": "a" }, { "repo": "b" }],
            "ticket_days": { "FIF-1": ["2026-08-03"] },
        });
        assert_eq!(sidecar_repo_count(&sidecar), 2);
    }

    #[test]
    fn repo_count_falls_back_to_ticket_days_for_older_sidecars() {
        let sidecar = serde_json::json!({ "ticket_days": { "FIF-1": [], "FIF-2": [] } });
        assert_eq!(sidecar_repo_count(&sidecar), 2);
    }

    #[test]
    fn repo_count_of_an_empty_sidecar_is_zero() {
        assert_eq!(sidecar_repo_count(&serde_json::json!({})), 0);
        assert_eq!(
            sidecar_repo_count(&serde_json::json!({ "facts": [], "ticket_days": {} })),
            0
        );
    }

    // ── dirty check ───────────────────────────────────────────────────────

    #[test]
    fn a_manual_run_never_skips_on_unchanged_inputs() {
        assert!(!should_skip_unchanged(TriggerSource::Manual, true));
    }

    #[test]
    fn a_machine_run_skips_on_unchanged_inputs() {
        for source in [
            TriggerSource::Scheduler,
            TriggerSource::SessionEnd,
            TriggerSource::AppStart,
            TriggerSource::SelfHeal,
        ] {
            assert!(should_skip_unchanged(source, true), "{source:?}");
            assert!(!should_skip_unchanged(source, false), "{source:?}");
        }
    }

    #[test]
    fn the_inputs_hash_covers_every_gathered_source() {
        let baseline = inputs_hash("facts", &Gathered::default());
        let variants = [
            (
                "notes",
                Gathered {
                    notes: "- note".into(),
                    ..Gathered::default()
                },
            ),
            (
                "conv",
                Gathered {
                    conv: Some("conv".into()),
                    ..Gathered::default()
                },
            ),
            (
                "prrev",
                Gathered {
                    prrev: Some("prrev".into()),
                    ..Gathered::default()
                },
            ),
            (
                "github",
                Gathered {
                    github: Some("github".into()),
                    ..Gathered::default()
                },
            ),
            (
                "claude_files",
                Gathered {
                    claude_files: vec!["a.rs".into()],
                    ..Gathered::default()
                },
            ),
            (
                "opencode",
                Gathered {
                    opencode_sessions: vec!["s".into()],
                    ..Gathered::default()
                },
            ),
            (
                "codex",
                Gathered {
                    codex_sessions: vec!["s".into()],
                    ..Gathered::default()
                },
            ),
            (
                "gemini",
                Gathered {
                    gemini_sessions: vec!["s".into()],
                    ..Gathered::default()
                },
            ),
            (
                "grok",
                Gathered {
                    grok_sessions: vec!["s".into()],
                    ..Gathered::default()
                },
            ),
        ];
        for (label, changed) in variants {
            assert_ne!(
                baseline,
                inputs_hash("facts", &changed),
                "a change to {label} must dirty the hash"
            );
        }
        assert_ne!(baseline, inputs_hash("other facts", &Gathered::default()));
        assert_eq!(baseline, inputs_hash("facts", &Gathered::default()));
    }

    // ── fallback decision table ───────────────────────────────────────────

    const GOOD_BODY: &str = "**autostand — FIF-133 — port**\n- Implemented the runner\n";

    fn tickets() -> Vec<String> {
        vec!["FIF-133".to_string()]
    }

    #[test]
    fn det_mode_never_consults_the_llm_body() {
        let decision = decide_render(RenderMode::Det, Some(&llm(GOOD_BODY)), &tickets(), &[]);
        assert_eq!(decision, RenderDecision::Deterministic);
        assert_eq!(decision.core_mode(), CoreMode::Det);
        assert_eq!(decision.render_used(), RenderUsed::Det);
        assert!(!decision.fellback());
        assert!(!decision.persists_hash());
        assert!(decision.llm_body().is_none());
    }

    #[test]
    fn a_valid_llm_body_is_accepted_verbatim() {
        let decision = decide_render(RenderMode::Auto, Some(&llm(GOOD_BODY)), &tickets(), &[]);
        assert_eq!(decision, RenderDecision::Accepted(GOOD_BODY.to_string()));
        assert_eq!(decision.llm_body(), Some(GOOD_BODY));
        // Core must not re-run its own heuristic over a body step (n) cleared.
        assert_eq!(decision.core_mode(), CoreMode::Llm);
        assert_eq!(decision.render_used(), RenderUsed::Llm);
        assert!(!decision.fellback());
        assert!(decision.persists_hash());
    }

    #[test]
    fn an_absent_llm_body_falls_back_without_failing() {
        let decision = decide_render(RenderMode::Auto, None, &tickets(), &[]);
        assert_eq!(decision, RenderDecision::Missing);
        assert_eq!(decision.core_mode(), CoreMode::Det);
        assert_eq!(decision.render_used(), RenderUsed::LlmFallback);
        assert!(decision.fellback());
        assert!(!decision.persists_hash());
    }

    #[test]
    fn llm_mode_also_falls_back_rather_than_writing_nothing() {
        // A configured preference must not leave a working day without a standup.
        let decision = decide_render(RenderMode::Llm, None, &tickets(), &[]);
        assert_eq!(decision.core_mode(), CoreMode::Det);
        assert_eq!(decision.render_used(), RenderUsed::LlmFallback);
        assert!(decision.fellback());
    }

    #[test]
    fn an_invalid_llm_body_is_rejected_with_a_stable_code() {
        let cases = [
            ("", "empty"),
            ("no bullets at all, just prose about FIF-133", "no_bullets"),
            ("- nothing to report\n", "no_work_claim"),
            ("- Shipped FIF-999\n", "invented_ticket"),
        ];
        for (body, expected) in cases {
            let decision = decide_render(RenderMode::Auto, Some(&llm(body)), &tickets(), &[]);
            match &decision {
                RenderDecision::Rejected { code, message } => {
                    assert_eq!(*code, expected, "{body:?}");
                    assert!(!message.is_empty(), "{body:?}");
                }
                other => panic!("{body:?} must be rejected, got {other:?}"),
            }
            assert_eq!(decision.core_mode(), CoreMode::Det);
            assert_eq!(decision.render_used(), RenderUsed::LlmFallback);
            assert!(decision.fellback());
            assert!(!decision.persists_hash());
        }
    }

    #[test]
    fn a_forbidden_ticket_is_reported_as_forbidden_not_invented() {
        let decision = decide_render(
            RenderMode::Auto,
            Some(&llm("- Shipped FIF-666\n")),
            &tickets(),
            &["FIF-666".to_string()],
        );
        match decision {
            RenderDecision::Rejected { code, .. } => assert_eq!(code, "forbidden_ticket"),
            other => panic!("expected a forbidden-ticket rejection, got {other:?}"),
        }
    }

    #[test]
    fn outcome_messages_name_the_provider_and_the_failure() {
        let body = llm(GOOD_BODY);
        assert_eq!(
            outcome_message(&RenderDecision::Deterministic, None),
            "deterministic render"
        );
        assert_eq!(
            outcome_message(&RenderDecision::Accepted(GOOD_BODY.into()), Some(&body)),
            "rendered by claude (sonnet)"
        );
        assert!(outcome_message(&RenderDecision::Missing, None).contains("deterministic"));
        let rejected = RenderDecision::Rejected {
            code: "empty",
            message: "empty render body".into(),
        };
        let message = outcome_message(&rejected, Some(&body));
        assert!(message.contains("empty render body"), "{message}");
        assert!(message.contains("deterministic"), "{message}");
    }

    // ── title + subtitle ──────────────────────────────────────────────────

    #[test]
    fn the_title_is_the_filing_date_in_month_day_year() {
        assert_eq!(title_for(d(2026, 8, 3)), "August 03, 2026");
        assert_eq!(title_for(d(2026, 12, 25)), "December 25, 2026");
    }

    #[test]
    fn the_subtitle_reads_like_the_spec_example() {
        // Mon 2026-08-03's window is Thu 07-30 .. Fri 07-31: same month, so the
        // month is written once and the second day follows the en-dash.
        assert_eq!(
            subtitle_for(&compute_window(d(2026, 8, 3))),
            "_Work completed July 30–31, 2026._"
        );
    }

    #[test]
    fn the_subtitle_separator_is_an_en_dash() {
        let subtitle = subtitle_for(&compute_window(d(2026, 8, 3)));
        assert!(subtitle.contains('\u{2013}'), "{subtitle}");
        assert!(!subtitle.contains('-'), "hyphen leaked in: {subtitle}");
    }

    #[test]
    fn a_cross_month_window_repeats_the_month() {
        assert_eq!(
            subtitle_for(&window(d(2026, 7, 31), d(2026, 8, 3))),
            "_Work completed July 31–August 03, 2026._"
        );
    }

    #[test]
    fn a_cross_year_window_spells_both_years() {
        assert_eq!(
            subtitle_for(&window(d(2025, 12, 31), d(2026, 1, 2))),
            "_Work completed December 31, 2025–January 02, 2026._"
        );
    }

    #[test]
    fn a_single_day_window_drops_the_range() {
        assert_eq!(
            subtitle_for(&window(d(2026, 8, 3), d(2026, 8, 3))),
            "_Work completed August 03, 2026._"
        );
    }

    #[test]
    fn the_subtitle_is_wrapped_in_italics() {
        let subtitle = subtitle_for(&compute_window(d(2026, 8, 3)));
        assert!(subtitle.starts_with("_Work completed "), "{subtitle}");
        assert!(subtitle.ends_with("._"), "{subtitle}");
    }

    // ── FACTS parsing ─────────────────────────────────────────────────────

    const FACTS: &str = "\
### repo: autostand / tickets: FIF-133, FIF-140 / commits (2):
- (FIF-133) Implemented the pipeline runner
- (-) Tidied the workspace lints

### repo: other / tickets: (none) / commits (1):
- (-) Bumped a dependency

<!-- TICKET_DAYS: FIF-133=2026-07-30,2026-07-31,FIF-666=2026-06-01 -->
";

    #[test]
    fn ticket_days_are_split_out_of_the_facts() {
        let (facts, days) = split_ticket_days(FACTS);
        assert!(!facts.contains("TICKET_DAYS"), "{facts}");
        assert!(facts.contains("### repo: autostand"));
        assert!(facts.ends_with("Bumped a dependency\n"), "{facts:?}");
        assert_eq!(
            days.get("FIF-133"),
            Some(&vec![d(2026, 7, 30), d(2026, 7, 31)])
        );
        assert_eq!(days.get("FIF-666"), Some(&vec![d(2026, 6, 1)]));
    }

    #[test]
    fn out_of_window_tickets_never_leak_into_the_facts() {
        // Regression guard: leaving the marker in would make FIF-666 (committed
        // in June) look like an in-window fact to provenance and to the LLM.
        let (facts, _) = split_ticket_days(FACTS);
        assert!(!facts.contains("FIF-666"), "{facts}");
    }

    #[test]
    fn facts_without_the_marker_survive_unchanged() {
        let (facts, days) = split_ticket_days("### repo: a / tickets: X-1 / commits (0):\n");
        assert_eq!(facts, "### repo: a / tickets: X-1 / commits (0):\n");
        assert!(days.is_empty());
    }

    #[test]
    fn empty_facts_stay_empty() {
        let (facts, days) = split_ticket_days("");
        assert!(facts.is_empty());
        assert!(days.is_empty());
    }

    #[test]
    fn repo_facts_are_reconstructed_from_the_text_block() {
        let (facts, _) = split_ticket_days(FACTS);
        let parsed = parse_repo_facts(&facts);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].repo, "autostand");
        assert_eq!(parsed[0].ticket.as_deref(), Some("FIF-133"));
        assert_eq!(parsed[0].title, "Implemented the pipeline runner");
        assert_eq!(parsed[0].commits.len(), 2);
        assert_eq!(parsed[0].commits[1].subject, "Tidied the workspace lints");
        assert_eq!(parsed[1].repo, "other");
        assert!(parsed[1].ticket.is_none(), "(none) is not a ticket key");
    }

    #[test]
    fn note_refs_are_grouped_by_their_dated_heading() {
        let notes = "## 2026-07-30\n- Drafted the spec\n- Reviewed the diagram\n\n## 2026-07-31\n- Wrote tests\n";
        let parsed = parse_note_refs(notes);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].date, "2026-07-30");
        assert_eq!(
            parsed[0].clauses,
            ["Drafted the spec", "Reviewed the diagram"]
        );
        assert_eq!(parsed[1].date, "2026-07-31");
        assert_eq!(parsed[1].clauses, ["Wrote tests"]);
    }

    #[test]
    fn undated_note_text_still_produces_a_reference() {
        let parsed = parse_note_refs("just a loose line\n");
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].date.is_empty());
        assert_eq!(parsed[0].clauses, ["just a loose line"]);
    }

    // ── result construction ───────────────────────────────────────────────

    fn outputs() -> CompileOutputs {
        CompileOutputs {
            body: "- did work".into(),
            render_used: CoreRenderModeUsed::Det,
            fellback: true,
            accumulated_count: 3,
            audit_path: Some(PathBuf::from("/state/audit/2026-08-03-host.json")),
        }
    }

    #[test]
    fn an_ok_result_carries_the_render_decision() {
        let result = ok_result(
            d(2026, 8, 3),
            "host",
            Path::new("/dailies/2026-08-03.md"),
            &RenderDecision::Missing,
            &outputs(),
            "llm unavailable",
        );
        assert_eq!(result.date, "2026-08-03");
        assert_eq!(result.host, "host");
        assert_eq!(result.status, CompileStatus::Ok);
        assert_eq!(result.render_used, RenderUsed::LlmFallback);
        assert!(result.fellback, "a fallback must be visible to the UI");
        assert_eq!(result.accumulated_count, 3);
        assert_eq!(result.file_path, "/dailies/2026-08-03.md");
        assert_eq!(
            result.audit_path.as_deref(),
            Some("/state/audit/2026-08-03-host.json")
        );
        assert_eq!(result.message, "llm unavailable");
    }

    #[test]
    fn a_clean_llm_result_does_not_report_a_fallback() {
        let result = ok_result(
            d(2026, 8, 3),
            "host",
            Path::new("/dailies/2026-08-03.md"),
            &RenderDecision::Accepted(GOOD_BODY.into()),
            &outputs(),
            "rendered by claude (sonnet)",
        );
        assert_eq!(result.render_used, RenderUsed::Llm);
        assert!(!result.fellback);
    }

    #[test]
    fn a_skip_result_still_points_at_the_file() {
        let result = skip_result(
            d(2026, 8, 3),
            "host",
            Path::new("/dailies/2026-08-03.md"),
            "frozen",
        );
        assert_eq!(result.status, CompileStatus::Skip);
        assert_eq!(result.message, "frozen");
        assert_eq!(result.file_path, "/dailies/2026-08-03.md");
        assert_eq!(result.accumulated_count, 0);
        assert!(result.audit_path.is_none());
    }

    #[test]
    fn an_error_result_has_no_file_path() {
        let result = error_result(d(2026, 8, 3), "host", "disk full");
        assert_eq!(result.status, CompileStatus::Error);
        assert!(
            result.file_path.is_empty(),
            "an error wrote nothing, so there is nothing to open"
        );
        assert_eq!(result.message, "disk full");
    }

    #[test]
    fn every_result_serializes_the_pinned_wire_shape() {
        let value = serde_json::to_value(base_result(
            d(2026, 8, 3),
            "host",
            Path::new("/dailies/2026-08-03.md"),
        ))
        .expect("serializes");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "accumulated_count",
                "audit_path",
                "date",
                "fellback",
                "file_path",
                "host",
                "message",
                "render_used",
                "status",
            ]
        );
        assert_eq!(value["status"], serde_json::json!("ok"));
    }

    // ── steps + triggers ──────────────────────────────────────────────────

    #[test]
    fn progress_percentages_are_monotonic_and_bounded() {
        let mut previous = 0;
        for step in STEPS {
            let percent = step.percent();
            assert!(percent > previous, "{} went backwards", step.label());
            assert!(percent <= 100, "{}", step.label());
            previous = percent;
        }
        assert_eq!(Step::Done.percent(), 100);
        // Pinned by the event example in docs/tauri/02-ipc-contracts.md.
        assert_eq!(Step::RenderLlm.percent(), 72);
    }

    #[test]
    fn step_labels_are_snake_case_and_unique() {
        let mut labels: Vec<&str> = STEPS.into_iter().map(Step::label).collect();
        labels.sort_unstable();
        let total = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), total, "duplicate step label");
        for label in labels {
            assert!(
                !label.is_empty() && label.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{label} is not snake_case"
            );
        }
    }

    #[test]
    fn steps_map_onto_the_documented_state_machine() {
        assert_eq!(Step::Gather.state_kind(), PipelineStateKind::Gathering);
        assert_eq!(Step::RenderLlm.state_kind(), PipelineStateKind::Rendering);
        assert_eq!(Step::Write.state_kind(), PipelineStateKind::Rendering);
        assert_eq!(Step::Done.state_kind(), PipelineStateKind::Done);
    }

    #[test]
    fn trigger_sources_map_onto_the_three_ipc_buckets() {
        assert_eq!(last_trigger(TriggerSource::Manual), LastTrigger::Manual);
        assert_eq!(last_trigger(TriggerSource::SelfHeal), LastTrigger::SelfHeal);
        for source in [
            TriggerSource::Scheduler,
            TriggerSource::SessionEnd,
            TriggerSource::AppStart,
        ] {
            assert_eq!(last_trigger(source), LastTrigger::Scheduled, "{source:?}");
        }
    }

    #[test]
    fn self_heal_defaults_to_enabled() {
        // The gate in `trigger` reads this flag; a default of `false` would mean
        // a missed day is never backfilled out of the box.
        assert!(SchedulerConfig::default().self_heal);
    }

    // ── git root ──────────────────────────────────────────────────────────

    #[test]
    fn the_git_root_is_the_dailies_dir_when_it_is_the_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".git")).expect("mkdir");
        assert_eq!(git_root_for(dir.path()), dir.path());
    }

    #[test]
    fn the_git_root_is_found_above_a_nested_dailies_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".git")).expect("mkdir");
        let dailies = dir.path().join("dailies");
        std::fs::create_dir_all(&dailies).expect("mkdir");
        assert_eq!(git_root_for(&dailies), dir.path());
    }

    #[test]
    fn a_dailies_dir_outside_any_repo_walks_up_and_terminates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dailies = dir.path().join("standups");
        std::fs::create_dir_all(&dailies).expect("mkdir");
        let root = git_root_for(&dailies);
        assert!(
            dailies.starts_with(&root),
            "{} must contain {}",
            root.display(),
            dailies.display()
        );
    }

    // ── audit patch ───────────────────────────────────────────────────────

    #[test]
    fn the_audit_patch_fills_the_fields_core_cannot_know() {
        let (facts, _) = split_ticket_days(FACTS);
        let gathered = Gathered {
            notes: "## 2026-07-30\n- Drafted the spec\n".into(),
            github: Some("- opened PR #1".into()),
            failures: vec![("github".into(), "gh not on PATH".into())],
            ..Gathered::default()
        };
        let body = llm(GOOD_BODY);

        let patch = audit_patch(
            &compute_window(d(2026, 8, 3)),
            &facts,
            &gathered,
            &RenderDecision::Accepted(GOOD_BODY.into()),
            Some(&body),
            "deadbeef",
        );
        // `start` / `end` are core's key names — commands::standup reads them.
        assert_eq!(patch["window"]["start"], serde_json::json!("2026-07-30"));
        assert_eq!(patch["window"]["end"], serde_json::json!("2026-07-31"));
        assert_eq!(patch["facts"].as_array().expect("facts").len(), 2);
        assert_eq!(patch["notes"].as_array().expect("notes").len(), 1);
        assert_eq!(patch["render_used"], serde_json::json!("llm"));
        assert_eq!(patch["fellback"], serde_json::json!(false));
        assert_eq!(patch["provider"], serde_json::json!("claude"));
        assert_eq!(patch["model"], serde_json::json!("sonnet"));
        assert_eq!(patch["hash"], serde_json::json!("deadbeef"));
        assert_eq!(patch["validation_failure"], serde_json::Value::Null);
        assert_eq!(
            patch["gather_failures"][0]["source"],
            serde_json::json!("github")
        );
    }

    #[test]
    fn the_audit_patch_records_a_validation_failure() {
        let patch = audit_patch(
            &compute_window(d(2026, 8, 3)),
            "",
            &Gathered::default(),
            &RenderDecision::Rejected {
                code: "invented_ticket",
                message: "render invents ticket FIF-999".into(),
            },
            None,
            "cafe",
        );
        assert_eq!(patch["render_used"], serde_json::json!("llm_fallback"));
        assert_eq!(patch["fellback"], serde_json::json!(true));
        assert_eq!(patch["provider"], serde_json::Value::Null);
        assert_eq!(
            patch["validation_failure"]["code"],
            serde_json::json!("invented_ticket")
        );
    }

    // ── headless config ───────────────────────────────────────────────────

    #[test]
    fn the_headless_config_path_matches_the_stores_own_layout() {
        // `tauri-plugin-store` resolves a relative path against
        // `app_data_dir()` = `dirs::data_dir()/<bundle identifier>`. If this
        // drifts, a scheduled run silently compiles with default settings.
        assert_eq!(
            config_store_path(Some(Path::new("/home/t/.local/share"))),
            Some(PathBuf::from(
                "/home/t/.local/share/com.miguel50flowers.autostand/config.json"
            ))
        );
        assert_eq!(config_store_path(None), None);
    }

    /// The bytes `tauri-plugin-store` would have written for `config`.
    fn store_bytes(config: &AppConfig) -> Vec<u8> {
        serde_json::json!({
            "config": serde_json::to_value(config).expect("AppConfig serializes"),
            "unrelated-plugin-key": 7,
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn the_headless_loader_reads_the_config_out_of_the_store_map() {
        let stored = AppConfig {
            dailies_dir: "/srv/dailies".to_string(),
            jira_base: "https://j.example".to_string(),
            ..AppConfig::default()
        };
        let config = config_from_store_bytes(&store_bytes(&stored)).expect("store parses");
        assert_eq!(config.dailies_dir, "/srv/dailies");
        assert_eq!(config.jira_base, "https://j.example");
        assert_eq!(config.render_mode, stored.render_mode);
    }

    #[test]
    fn a_store_without_a_config_entry_yields_defaults() {
        // The store exists as soon as any plugin key is written; the Settings
        // page may never have saved a config.
        let config = config_from_store_bytes(b"{}").expect("empty store parses");
        assert_eq!(config.dailies_dir, AppConfig::default().dailies_dir);
    }

    #[test]
    fn a_corrupt_store_is_an_error_not_a_silent_default() {
        // Falling back to defaults here would file standups into a directory the
        // user never configured, from a run with no UI to notice.
        assert!(matches!(
            config_from_store_bytes(b"{ not json"),
            Err(AppError::Config(_))
        ));
        assert!(matches!(
            config_from_store_bytes(br#"{"config": 42}"#),
            Err(AppError::Config(_))
        ));
    }

    #[test]
    fn the_headless_loader_survives_a_machine_that_never_ran_the_app() {
        let dir = tempfile::tempdir().expect("temp data dir");
        // No store file at all: a first-ever scheduled run must still compile.
        let config = load_config_from_disk(Some(dir.path())).expect("defaults");
        assert_eq!(config.dailies_dir, AppConfig::default().dailies_dir);

        let stored = AppConfig {
            dailies_dir: "/srv/d".to_string(),
            ..AppConfig::default()
        };
        let store = config_store_path(Some(dir.path())).expect("path");
        std::fs::create_dir_all(store.parent().expect("parent")).expect("mkdir");
        std::fs::write(&store, store_bytes(&stored)).expect("write");
        assert_eq!(
            load_config_from_disk(Some(dir.path()))
                .expect("stored config")
                .dailies_dir,
            "/srv/d"
        );
    }
}
