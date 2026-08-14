//! Gather orchestration: run the enabled data sources over a window.
//!
//! See step (e) of `docs/specs/pipeline.md`.
//!
//! This module owns steps (b), (c) and (e): it runs every enabled
//! [`DataSource`] concurrently over the compile window, caches each successful
//! result with the 2700 s TTL from `docs/specs/pipeline.md` § Cache, and merges
//! the per-source [`SourceData`] into the flat shape
//! `autostand_core::pipeline::CompileInputs` consumes.
//!
//! Two rules from § Error recovery are structural here rather than optional:
//! a source that errors or is unavailable is **skipped and recorded**, never
//! fatal; and `local-git` is authoritative, so it runs even when the persisted
//! config has its flag switched off.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use autostand_adapters::sources::{
    claude_code::ClaudeCodeDataSource, codex::CodexDataSource, gemini_cli::GeminiCliDataSource,
    github::GithubDataSource, grok_cli::GrokCliDataSource, local_git::LocalGitDataSource,
    opencode::OpenCodeDataSource, remember::RememberDataSource, DataSource, DataSourceConfig,
    DateWindow, SourceData,
};

use crate::commands::types::{AppConfig, DataSourceConfigs};
use crate::error::AppError;

/// Cache TTL from `docs/specs/pipeline.md` § Cache — 2700 s (45 min).
pub const CACHE_TTL: Duration = Duration::from_secs(2700);

/// Step every gather line is filed under, shared with the adapters.
const GATHER_STEP: &str = "gather";

/// Directory (relative to the state dir) holding the per-source gather cache.
const CACHE_SUBDIR: &str = "cache";

/// `DataSource::id()` of the authoritative commit scanner.
const SOURCE_LOCAL_GIT: &str = "local-git";
/// `DataSource::id()` of the `gh`-backed GitHub source.
const SOURCE_GITHUB: &str = "github";
/// `DataSource::id()` of the Claude Code session source.
const SOURCE_CLAUDE_CODE: &str = "claude-code";
/// `DataSource::id()` of the remember-plugin note reader.
const SOURCE_REMEMBER: &str = "remember";
/// `DataSource::id()` of the `OpenCode` session source.
const SOURCE_OPENCODE: &str = "opencode";
/// `DataSource::id()` of the Codex CLI session source.
const SOURCE_CODEX: &str = "codex";
/// `DataSource::id()` of the Gemini CLI session source.
const SOURCE_GEMINI: &str = "gemini-cli";
/// `DataSource::id()` of the Grok CLI session source.
const SOURCE_GROK: &str = "grok-cli";

/// Every source id this orchestrator knows, in merge order.
///
/// The order is the order results are folded into [`Gathered`], not the order
/// they run in (they all run concurrently). `local-git` is first because its
/// FACTS are what the anti-regression guard keys on.
const SOURCE_ORDER: &[&str] = &[
    SOURCE_LOCAL_GIT,
    SOURCE_REMEMBER,
    SOURCE_GITHUB,
    SOURCE_CLAUDE_CODE,
    SOURCE_OPENCODE,
    SOURCE_CODEX,
    SOURCE_GEMINI,
    SOURCE_GROK,
];

/// Heading the GitHub adapter puts above its PR-review bullets.
const REVIEW_HEADING: &str = "### Review comments";

/// Heading the GitHub adapter puts at the top of its enrichment block.
///
/// Stripped on merge: `autostand_core::deterministic::render_det` emits this
/// heading itself, so keeping it would render it twice.
const GITHUB_HEADING: &str = "## GITHUB ACTIVITY";

/// Marker the adapters use for an empty section.
const EMPTY_SECTION: &str = "_(none)_";

/// Everything gathered for one compile window, flattened into the fields
/// `autostand_core::pipeline::CompileInputs` needs.
#[derive(Debug, Clone, Default)]
pub struct Gathered {
    /// FACTS block: per-repo commits from `local-git` (authoritative).
    pub facts: String,
    /// Narrative notes from the remember plugin, pre-scrub.
    pub notes: String,
    /// GitHub PR activity (PRs opened/merged), heading stripped.
    pub github: Option<String>,
    /// Claude Code conversation digest.
    pub conv: Option<String>,
    /// PR review bullets, split out of the GitHub source's block.
    pub prrev: Option<String>,
    /// Repo-attributed files Claude Code wrote.
    pub claude_files: Vec<String>,
    /// `OpenCode` session digest entries followed by its attributed files.
    pub opencode_sessions: Vec<String>,
    /// Codex session digest entries followed by its attributed files.
    pub codex_sessions: Vec<String>,
    /// Gemini CLI session digest entries followed by its attributed files.
    pub gemini_sessions: Vec<String>,
    /// Grok CLI session digest entries followed by its attributed files.
    pub grok_sessions: Vec<String>,
    /// (`source_id`, error) for sources that failed — recorded, never fatal.
    pub failures: Vec<(String, String)>,
}

/// Convert the core window (step (a) of the pipeline) into the adapter-facing
/// [`DateWindow`].
///
/// The two types carry the same three values under different names; the
/// conversion lives here because this module is the boundary between the pure
/// core pipeline and the async adapter layer.
pub fn to_date_window(window: &autostand_core::dates::Window) -> DateWindow {
    DateWindow {
        start: window.range_start,
        end: window.range_end,
        dates: window.dates.clone(),
    }
}

/// Run every enabled data source over `window` and merge the results.
///
/// Sources run concurrently (one Tokio task each) because they shell out to
/// `git`/`gh` and read session stores; serial gathering would make a compile
/// take as long as the sum of every source. Results are folded back in the
/// fixed [`SOURCE_ORDER`] so a compile is reproducible regardless of which
/// source finishes first.
///
/// Never returns an error: a failing source is recorded in
/// [`Gathered::failures`] and the compile continues with what it has.
#[tracing::instrument(skip_all, fields(start = %window.start, end = %window.end))]
pub async fn gather_all(window: &DateWindow, config: &AppConfig, state_dir: &Path) -> Gathered {
    let ds_config = source_config(config);
    let key = window_hash(window);
    let ids = enabled_source_ids(&config.data_sources);

    let mut handles = Vec::with_capacity(ids.len());
    for id in ids {
        let window = window.clone();
        let ds_config = ds_config.clone();
        let state_dir = state_dir.to_path_buf();
        let key = key.clone();
        handles.push((
            id,
            // `inherit` is load-bearing, not decoration: task-locals do not cross
            // `tokio::spawn`, and every `git`/`gh` this pipeline runs lives inside
            // one of these tasks. Without it the whole gather — the loudest part of
            // a compile — logs into the null sink and the Terminal shows nothing
            // between "gathering" and "rendering".
            tokio::spawn(autostand_runlog::inherit(async move {
                gather_one(id, &window, &ds_config, &state_dir, &key).await
            })),
        ));
    }

    let mut out = Gathered::default();
    for (id, handle) in handles {
        match handle.await {
            Ok(Ok(data)) => merge_source_data(&mut out, id, data),
            Ok(Err(message)) => {
                tracing::warn!(source = id, error = %message, "data source skipped");
                out.failures.push((id.to_string(), message));
            }
            Err(join_error) => {
                // A panicking source must not take the compile down with it.
                let message = format!("gather task failed: {join_error}");
                tracing::warn!(source = id, error = %message, "data source panicked");
                out.failures.push((id.to_string(), message));
            }
        }
    }

    tracing::info!(
        facts = out.facts.len(),
        notes = out.notes.len(),
        failures = out.failures.len(),
        "gather complete"
    );
    out
}

/// Gather one source, consulting (and refreshing) the TTL cache.
///
/// Returns `Err(message)` when the source is unknown, unavailable or errored —
/// the caller records it as a failure and moves on.
async fn gather_one(
    id: &str,
    window: &DateWindow,
    config: &DataSourceConfig,
    state_dir: &Path,
    window_key: &str,
) -> Result<SourceData, String> {
    if let Some(cached) = read_cache(state_dir, id, window_key) {
        tracing::debug!(source = id, "gather cache hit");
        autostand_runlog::log(
            autostand_runlog::LogLine::done(GATHER_STEP, format!("{id} (cached)"))
                .with_detail("no subprocess"),
        );
        return Ok(cached);
    }
    let Some(source) = make_source(id) else {
        return Err(format!("unknown data source id: {id}"));
    };
    if !is_available(id, source.as_ref(), config) {
        return Err(format!("{} is not available", source.display_name()));
    }
    // The source's own spawns are silent per call (they run per repository);
    // this line is what tells the user which source is working right now.
    autostand_runlog::info(GATHER_STEP, format!("reading {id}"));
    let data = source
        .gather(window, config)
        .await
        .map_err(|e| e.to_string())?;

    // Only Ok results are cached (spec § Cache); a cache write failure is not
    // worth failing an otherwise-successful gather over.
    if let Err(e) = write_cache(state_dir, id, window_key, &data) {
        tracing::warn!(source = id, error = %e, "gather cache write failed");
    }
    Ok(data)
}

/// Whether `source` can run for this config.
///
/// `local-git` and `remember` re-derive `github_dir` from the environment in
/// their own `is_available`, which ignores the path the user configured in the
/// app; for those two we ask the configured directory instead so a custom
/// `github_dir` does not silently disable the authoritative source.
fn is_available(id: &str, source: &dyn DataSource, config: &DataSourceConfig) -> bool {
    match id {
        SOURCE_LOCAL_GIT | SOURCE_REMEMBER => config.github_dir.is_dir(),
        _ => source.is_available(),
    }
}

/// Build the data source for an id, or `None` if the id is unknown.
fn make_source(id: &str) -> Option<Box<dyn DataSource>> {
    let source: Box<dyn DataSource> = match id {
        SOURCE_LOCAL_GIT => Box::new(LocalGitDataSource),
        SOURCE_GITHUB => Box::new(GithubDataSource),
        SOURCE_CLAUDE_CODE => Box::new(ClaudeCodeDataSource),
        SOURCE_REMEMBER => Box::new(RememberDataSource),
        SOURCE_OPENCODE => Box::new(OpenCodeDataSource),
        SOURCE_CODEX => Box::new(CodexDataSource),
        SOURCE_GEMINI => Box::new(GeminiCliDataSource),
        SOURCE_GROK => Box::new(GrokCliDataSource),
        _ => return None,
    };
    Some(source)
}

/// The source ids to run, in [`SOURCE_ORDER`].
///
/// `local-git` is always included: it is authoritative, and a compile without
/// FACTS is indistinguishable from a transient git failure to the
/// anti-regression guard.
fn enabled_source_ids(flags: &DataSourceConfigs) -> Vec<&'static str> {
    SOURCE_ORDER
        .iter()
        .copied()
        .filter(|id| match *id {
            SOURCE_LOCAL_GIT => true,
            SOURCE_GITHUB => flags.github,
            SOURCE_CLAUDE_CODE => flags.claude_code,
            SOURCE_REMEMBER => flags.remember,
            SOURCE_OPENCODE => flags.opencode,
            SOURCE_CODEX => flags.codex,
            SOURCE_GEMINI => flags.gemini_cli,
            SOURCE_GROK => flags.grok_cli,
            _ => false,
        })
        .collect()
}

/// Project the persisted app config onto the adapter-facing
/// [`DataSourceConfig`].
///
/// Blank directory fields fall back to the same defaults the Settings UI shows,
/// because the UI clears a text field to `""` rather than to `null`.
pub fn source_config(config: &AppConfig) -> DataSourceConfig {
    let home = dirs::home_dir();
    DataSourceConfig {
        github_dir: crate::commands::repos::resolve_github_dir(
            Some(config.github_dir.as_str()),
            home.as_deref(),
        ),
        dailies_dir: crate::commands::standup::resolve_dailies_dir(
            Some(config.dailies_dir.as_str()),
            home.as_deref(),
        ),
        authors: config.standup_authors.clone(),
        git_refs: config.git_refs.clone(),
        gh_reviewer: config.review.reviewer.clone(),
        gh_pr_org: config.review.pr_org.clone(),
        gh_max_prs: usize::try_from(config.review.max_prs).unwrap_or(usize::MAX),
        gh_comment_len: usize::try_from(config.review.comment_len).unwrap_or(usize::MAX),
        jira_base: config.jira_base.clone(),
    }
}

/// Fold one source's [`SourceData`] into the accumulating [`Gathered`].
///
/// Pure so the merge rules can be tested without touching a real data source.
/// Unknown ids are logged and ignored rather than panicking — an id typo must
/// not be able to abort a compile.
pub fn merge_source_data(out: &mut Gathered, source_id: &str, data: SourceData) {
    match source_id {
        SOURCE_LOCAL_GIT => push_block(&mut out.facts, data.facts.as_deref()),
        SOURCE_REMEMBER => push_block(&mut out.notes, data.notes.as_deref()),
        SOURCE_GITHUB => {
            let (github, prrev) = split_github_enrichment(data.enrichment.as_deref());
            out.github = github;
            out.prrev = prrev;
        }
        SOURCE_CLAUDE_CODE => {
            let mut block = data.enrichment.unwrap_or_default();
            if !data.files.is_empty() {
                if !block.ends_with('\n') && !block.is_empty() {
                    block.push('\n');
                }
                block.push_str("files:\n");
                for file in &data.files {
                    let _ = std::fmt::Write::write_fmt(&mut block, format_args!("- {file}\n"));
                }
            }
            out.conv = non_empty(Some(&block));
            extend_unique(&mut out.claude_files, data.files);
        }
        SOURCE_OPENCODE => out.opencode_sessions = session_entries(&data),
        SOURCE_CODEX => out.codex_sessions = session_entries(&data),
        SOURCE_GEMINI => out.gemini_sessions = session_entries(&data),
        SOURCE_GROK => out.grok_sessions = session_entries(&data),
        other => tracing::warn!(source = other, "no merge rule for data source; dropped"),
    }
}

/// Session-source payload: one entry per prompt in the digest, then the files
/// that source attributed to a repo.
///
/// Both halves are strings the debug preview lists verbatim; keeping them in
/// one vec is what the `*_sessions` fields are declared as, and dropping either
/// half would lose gathered work.
fn session_entries(data: &SourceData) -> Vec<String> {
    let mut entries: Vec<String> = data
        .enrichment
        .as_deref()
        .map(digest_bullets)
        .unwrap_or_default();
    extend_unique(&mut entries, data.files.clone());
    entries
}

/// The `- ` bullet texts of a `## CONTEXT` digest block.
fn digest_bullets(enrichment: &str) -> Vec<String> {
    enrichment
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(str::trim)
        .filter(|text| !text.is_empty() && *text != EMPTY_SECTION)
        .map(String::from)
        .collect()
}

/// Split the GitHub adapter's single enrichment block into (GITHUB, PRREV).
///
/// The adapter emits PR activity and review comments in one block, but the
/// pipeline treats them as two enrichment sources and the renderer gives each
/// its own heading. The trailing `GH-RECENT-TICKETS` marker stays on the GITHUB
/// side: provenance parses it for ticket coverage.
fn split_github_enrichment(block: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(block) = block else {
        return (None, None);
    };
    let mut activity: Vec<&str> = Vec::new();
    let mut reviews: Vec<&str> = Vec::new();
    let mut in_reviews = false;

    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed == GITHUB_HEADING {
            continue;
        }
        if trimmed == REVIEW_HEADING {
            in_reviews = true;
            continue;
        }
        // The trailing HTML marker closes the review section.
        if in_reviews && trimmed.starts_with("<!--") {
            in_reviews = false;
        }
        if in_reviews {
            reviews.push(line);
        } else {
            activity.push(line);
        }
    }

    let reviews: Vec<&str> = reviews
        .into_iter()
        .filter(|l| l.trim() != EMPTY_SECTION)
        .collect();

    (
        non_empty(Some(activity.join("\n").as_str())),
        non_empty(Some(reviews.join("\n").as_str())),
    )
}

/// `Some(trimmed)` when the text has content, `None` otherwise.
fn non_empty(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
    // NOTE: returns the trimmed text so downstream `Option::is_some` checks
    // never fire on whitespace-only enrichment.
}

/// Append `block` to `dst`, separating with a blank line when `dst` has content.
fn push_block(dst: &mut String, block: Option<&str>) {
    let Some(block) = block.map(str::trim).filter(|b| !b.is_empty()) else {
        return;
    };
    if !dst.is_empty() {
        dst.push_str("\n\n");
    }
    dst.push_str(block);
}

/// Append the items of `extra` that `dst` does not already contain.
fn extend_unique(dst: &mut Vec<String>, extra: Vec<String>) {
    for item in extra {
        if !dst.contains(&item) {
            dst.push(item);
        }
    }
}

// ---------------------------------------------------------------------------
// Cache (docs/specs/pipeline.md § Cache)
// ---------------------------------------------------------------------------

/// One cached gather result.
///
/// The timestamp lives inside the JSON rather than being read off the file's
/// mtime: mtime cannot be set from stable `std`, so an mtime-based TTL is not
/// testable, and a file copy (or a state dir restored from backup) would carry
/// a bogus age.
#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    /// When the entry was written.
    cached_at: DateTime<Utc>,
    /// The cached source payload.
    data: SourceData,
}

/// Stable cache key for a window: `sha256(range_start, range_end)`.
///
/// Keyed on the range only, per spec — the dates in between are derived from
/// it, and a config change clears the whole cache rather than re-keying it.
pub fn window_hash(window: &DateWindow) -> String {
    autostand_core::hashes::input_hash(&[
        window.start.to_string().as_str(),
        window.end.to_string().as_str(),
    ])
}

/// Path of a cached gather result:
/// `<state_dir>/cache/<source_id>/<window_hash>.json`.
pub fn cache_path(state_dir: &Path, source_id: &str, window_hash: &str) -> PathBuf {
    state_dir
        .join(CACHE_SUBDIR)
        .join(source_id)
        .join(format!("{window_hash}.json"))
}

/// Read a cached gather result, or `None` when it is missing, unreadable,
/// corrupt or older than [`CACHE_TTL`].
///
/// Every failure mode collapses to `None` on purpose: a bad cache entry must
/// cost one re-gather, never a failed compile.
pub fn read_cache(state_dir: &Path, source_id: &str, window_hash: &str) -> Option<SourceData> {
    let path = cache_path(state_dir, source_id, window_hash);
    let bytes = std::fs::read(&path).ok()?;
    let entry: CacheEntry = serde_json::from_slice(&bytes).ok()?;
    if is_fresh(entry.cached_at, Utc::now()) {
        Some(entry.data)
    } else {
        None
    }
}

/// Write a gather result to the cache: atomic, mode `0600`.
///
/// Mode `0600` because a cached digest can contain (redacted, but still
/// private) conversation text.
pub fn write_cache(
    state_dir: &Path,
    source_id: &str,
    window_hash: &str,
    data: &SourceData,
) -> Result<(), AppError> {
    write_cache_at(state_dir, source_id, window_hash, data, Utc::now())
}

/// [`write_cache`] with an explicit timestamp, so TTL expiry is testable.
fn write_cache_at(
    state_dir: &Path,
    source_id: &str,
    window_hash: &str,
    data: &SourceData,
    cached_at: DateTime<Utc>,
) -> Result<(), AppError> {
    let path = cache_path(state_dir, source_id, window_hash);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let entry = CacheEntry {
        cached_at,
        data: data.clone(),
    };
    let json = serde_json::to_string(&entry)?;
    autostand_core::fileops::write_atomic(&path, &json)
        .map_err(|e| AppError::Io(format!("cache write {}: {e}", path.display())))?;
    set_mode_0600(&path)?;
    Ok(())
}

/// Drop every cached gather result.
///
/// Spec § Cache: any `set_config` invalidates the whole cache, since a changed
/// `github_dir` or author list makes every window key stale without changing it.
pub fn clear_cache(state_dir: &Path) -> Result<(), AppError> {
    let dir = state_dir.join(CACHE_SUBDIR);
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir)?;
    tracing::info!(dir = %dir.display(), "gather cache cleared");
    Ok(())
}

/// Whether an entry written at `cached_at` is still within [`CACHE_TTL`] at `now`.
///
/// A timestamp in the future (clock skew, edited file) counts as stale:
/// re-gathering is always safe, trusting a bogus timestamp is not.
fn is_fresh(cached_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    let ttl = i64::try_from(CACHE_TTL.as_secs()).unwrap_or(i64::MAX);
    let age = now.signed_duration_since(cached_at).num_seconds();
    (0..=ttl).contains(&age)
}

#[cfg(unix)]
fn set_mode_0600(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn set_mode_0600(_path: &Path) -> std::io::Result<()> {
    // Windows has no POSIX mode bits; the state dir itself is user-scoped.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        cache_path, clear_cache, digest_bullets, enabled_source_ids, gather_all, is_fresh,
        make_source, merge_source_data, non_empty, push_block, read_cache, session_entries,
        source_config, split_github_enrichment, to_date_window, window_hash, write_cache,
        write_cache_at, Gathered, CACHE_TTL, SOURCE_ORDER,
    };
    use crate::commands::types::{AppConfig, DataSourceConfigs, ReviewConfig};
    use autostand_adapters::sources::{DateWindow, SourceData};
    use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
    use std::path::{Path, PathBuf};

    /// A unique temp dir for one test. Hermetic: no HOME, no network, no repo.
    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "autostand-gather-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, d).expect("valid date")
    }

    fn test_window() -> DateWindow {
        DateWindow {
            start: day(1),
            end: day(2),
            dates: vec![day(1), day(2)],
        }
    }

    fn data_with_facts(facts: &str) -> SourceData {
        SourceData {
            facts: Some(facts.to_string()),
            ..SourceData::default()
        }
    }

    // -- terminal visibility ------------------------------------------------

    /// The trap this whole migration turns on: every source runs in its own
    /// `tokio::spawn`, and task-locals do not cross one. If the `inherit` wrapper
    /// in `gather_all` is ever dropped, the gather — every `git log`, every `gh`
    /// call — vanishes from the Terminal while still working perfectly, which is
    /// exactly the kind of regression no other test would catch.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_spawned_source_still_logs_into_the_run() {
        let state_dir = temp_dir("inherit");
        let window = test_window();
        let key = window_hash(&window);
        // Cached, so the source answers without touching the machine's repos.
        write_cache(&state_dir, "local-git", &key, &data_with_facts("FACTS"))
            .expect("seed the cache");

        let config = AppConfig {
            data_sources: DataSourceConfigs {
                github: false,
                claude_code: false,
                remember: false,
                opencode: false,
                codex: false,
                gemini_cli: false,
                grok_cli: false,
                ..DataSourceConfigs::default()
            },
            ..AppConfig::default()
        };

        let sink = std::sync::Arc::new(autostand_runlog::RecordingSink::new());
        let as_ref: autostand_runlog::SinkRef = sink.clone();
        let gathered = autostand_runlog::scoped(as_ref, async {
            gather_all(&window, &config, &state_dir).await
        })
        .await;

        assert_eq!(gathered.facts, "FACTS");
        let messages = sink.messages().join("\n");
        assert!(
            messages.contains("local-git"),
            "a bare tokio::spawn would have swallowed this: {messages:?}"
        );
        let _ = clear_cache(&state_dir);
    }

    // -- source_config -----------------------------------------------------

    #[test]
    fn source_config_maps_every_configured_field() {
        let config = AppConfig {
            github_dir: "/srv/repos".into(),
            dailies_dir: "/srv/dailies".into(),
            standup_authors: vec!["dev@example.com".into(), "dev".into()],
            git_refs: "--branches".into(),
            jira_base: "https://jira.example.com/browse".into(),
            review: ReviewConfig {
                reviewer: "dev".into(),
                pr_org: "example-org".into(),
                max_prs: 7,
                comment_len: 120,
                include_self_reviews: false,
            },
            ..AppConfig::default()
        };

        let ds = source_config(&config);
        assert_eq!(ds.github_dir, PathBuf::from("/srv/repos"));
        assert_eq!(ds.dailies_dir, PathBuf::from("/srv/dailies"));
        assert_eq!(ds.authors, vec!["dev@example.com", "dev"]);
        assert_eq!(ds.git_refs, "--branches");
        assert_eq!(ds.jira_base, "https://jira.example.com/browse");
        assert_eq!(ds.gh_reviewer, "dev");
        assert_eq!(ds.gh_pr_org, "example-org");
        assert_eq!(ds.gh_max_prs, 7);
        assert_eq!(ds.gh_comment_len, 120);
    }

    #[test]
    fn source_config_never_yields_a_relative_scan_root() {
        // A blank `github_dir` must not turn into `PathBuf::from("")`, which
        // would send every source scanning the process working directory.
        let config = AppConfig::default();
        let ds = source_config(&config);
        assert_ne!(ds.github_dir, PathBuf::new());
        assert_ne!(ds.dailies_dir, PathBuf::new());
    }

    // -- source registry ---------------------------------------------------

    #[test]
    fn every_ordered_id_builds_a_source_that_reports_that_id() {
        for id in SOURCE_ORDER {
            let source = make_source(id).unwrap_or_else(|| panic!("no source for {id}"));
            assert_eq!(source.id(), *id, "adapter id drifted from the merge order");
        }
    }

    #[test]
    fn unknown_source_ids_build_nothing() {
        assert!(make_source("remember-plugin").is_none());
        assert!(make_source("local_git").is_none());
        assert!(make_source("").is_none());
    }

    #[test]
    fn local_git_runs_even_when_its_flag_is_off() {
        let flags = DataSourceConfigs {
            local_git: false,
            github: false,
            claude_code: false,
            remember: false,
            opencode: false,
            codex: false,
            gemini_cli: false,
            grok_cli: false,
        };
        assert_eq!(enabled_source_ids(&flags), vec!["local-git"]);
    }

    #[test]
    fn only_enabled_sources_run_and_they_run_in_merge_order() {
        let flags = DataSourceConfigs {
            local_git: true,
            github: true,
            claude_code: false,
            remember: true,
            opencode: false,
            codex: false,
            gemini_cli: false,
            grok_cli: true,
        };
        assert_eq!(
            enabled_source_ids(&flags),
            vec!["local-git", "remember", "github", "grok-cli"]
        );
    }

    #[test]
    fn the_default_flag_set_enables_the_documented_sources() {
        assert_eq!(
            enabled_source_ids(&DataSourceConfigs::default()),
            vec!["local-git", "remember", "github", "claude-code", "opencode"]
        );
    }

    // -- merge -------------------------------------------------------------

    #[test]
    fn local_git_populates_facts_only() {
        let mut out = Gathered::default();
        merge_source_data(
            &mut out,
            "local-git",
            data_with_facts("### repo: autostand / tickets: FIF-1 / commits (1):\n- (FIF-1) x"),
        );
        assert!(out.facts.contains("FIF-1"));
        assert!(out.notes.is_empty());
        assert!(out.github.is_none());
        assert!(out.failures.is_empty());
    }

    #[test]
    fn remember_populates_notes_only() {
        let mut out = Gathered::default();
        merge_source_data(
            &mut out,
            "remember",
            SourceData {
                notes: Some("## 2026-08-01\n- drafted the spec".into()),
                ..SourceData::default()
            },
        );
        assert!(out.notes.contains("drafted the spec"));
        assert!(out.facts.is_empty());
    }

    #[test]
    fn blank_payloads_do_not_create_empty_sections() {
        let mut out = Gathered::default();
        merge_source_data(&mut out, "local-git", data_with_facts("   \n\n"));
        merge_source_data(
            &mut out,
            "claude-code",
            SourceData {
                enrichment: Some("  \n".into()),
                ..SourceData::default()
            },
        );
        assert!(out.facts.is_empty());
        assert!(out.conv.is_none());
    }

    #[test]
    fn github_splits_pr_activity_from_review_comments() {
        let block = "## GITHUB ACTIVITY\n\
             ### PRs opened\n\
             - org/repo #12 — \"Add IPC\" (opened 2026-08-01)\n\
             ### PRs merged\n\
             _(none)_\n\
             ### Review comments\n\
             - org/repo #9: \"nit: rename this\"\n\
             <!-- GH-RECENT-TICKETS: FIF-1,FIF-2 -->\n";
        let mut out = Gathered::default();
        merge_source_data(
            &mut out,
            "github",
            SourceData {
                enrichment: Some(block.into()),
                ..SourceData::default()
            },
        );

        let github = out.github.expect("github activity");
        assert!(github.contains("#12"));
        assert!(github.contains("GH-RECENT-TICKETS"));
        assert!(
            !github.contains("## GITHUB ACTIVITY"),
            "the renderer adds that heading itself: {github}"
        );
        assert!(!github.contains("nit: rename this"));

        let prrev = out.prrev.expect("review comments");
        assert_eq!(prrev, "- org/repo #9: \"nit: rename this\"");
        assert!(!prrev.contains("### Review comments"));
    }

    #[test]
    fn github_without_reviews_leaves_prrev_unset() {
        let block = "## GITHUB ACTIVITY\n\
             ### PRs opened\n\
             - org/repo #12 — \"Add IPC\" (opened 2026-08-01)\n\
             ### Review comments\n\
             _(none)_\n\
             <!-- GH-RECENT-TICKETS:  -->\n";
        let (github, prrev) = split_github_enrichment(Some(block));
        assert!(github.expect("activity").contains("#12"));
        assert!(prrev.is_none());
    }

    #[test]
    fn a_missing_github_block_splits_into_nothing() {
        assert_eq!(split_github_enrichment(None), (None, None));
        assert_eq!(split_github_enrichment(Some("   ")), (None, None));
    }

    #[test]
    fn claude_code_populates_conv_and_files() {
        let mut out = Gathered::default();
        merge_source_data(
            &mut out,
            "claude-code",
            SourceData {
                enrichment: Some("## CONTEXT\nprompts:\n- wired the pipeline\n".into()),
                files: vec!["autostand/gather.rs".into(), "autostand/gather.rs".into()],
                ..SourceData::default()
            },
        );
        assert!(out.conv.expect("conv").contains("wired the pipeline"));
        assert_eq!(out.claude_files, vec!["autostand/gather.rs"]);
    }

    #[test]
    fn session_sources_land_in_their_own_lists() {
        let payload = SourceData {
            enrichment: Some("## CONTEXT\nprompts:\n- refactored the queue\n".into()),
            files: vec!["autostand/queue.rs".into()],
            ..SourceData::default()
        };
        for (id, pick) in [
            ("opencode", 0_usize),
            ("codex", 1),
            ("gemini-cli", 2),
            ("grok-cli", 3),
        ] {
            let mut out = Gathered::default();
            merge_source_data(&mut out, id, payload.clone());
            let lists = [
                &out.opencode_sessions,
                &out.codex_sessions,
                &out.gemini_sessions,
                &out.grok_sessions,
            ];
            for (idx, list) in lists.iter().enumerate() {
                if idx == pick {
                    assert_eq!(
                        **list,
                        vec![
                            "refactored the queue".to_string(),
                            "autostand/queue.rs".to_string()
                        ],
                        "{id} did not fill its own list"
                    );
                } else {
                    assert!(list.is_empty(), "{id} leaked into another list");
                }
            }
            // Session sources must never contaminate the authoritative fields.
            assert!(out.facts.is_empty() && out.notes.is_empty() && out.conv.is_none());
        }
    }

    #[test]
    fn an_empty_session_digest_yields_no_entries() {
        let data = SourceData {
            enrichment: Some("## CONTEXT\nprompts:\n_(none)_\n".into()),
            ..SourceData::default()
        };
        assert!(session_entries(&data).is_empty());
    }

    #[test]
    fn digest_bullets_keeps_order_and_drops_non_bullets() {
        let digest = "## CONTEXT\nplans:\n- plan alpha\nprompts:\n- did a thing\n<!-- note -->\n";
        assert_eq!(digest_bullets(digest), vec!["plan alpha", "did a thing"]);
    }

    #[test]
    fn an_unknown_source_id_is_dropped_without_panicking() {
        let mut out = Gathered::default();
        merge_source_data(&mut out, "remember-plugin", data_with_facts("ignored"));
        assert!(out.facts.is_empty());
        assert!(out.failures.is_empty());
    }

    #[test]
    fn push_block_separates_repeated_contributions() {
        let mut dst = String::new();
        push_block(&mut dst, Some("first"));
        push_block(&mut dst, Some("  \n "));
        push_block(&mut dst, None);
        push_block(&mut dst, Some("second"));
        assert_eq!(dst, "first\n\nsecond");
    }

    #[test]
    fn non_empty_trims_and_rejects_whitespace() {
        assert_eq!(non_empty(Some("  hi  ")).as_deref(), Some("hi"));
        assert!(non_empty(Some(" \n\t ")).is_none());
        assert!(non_empty(None).is_none());
    }

    // -- cache -------------------------------------------------------------

    #[test]
    fn cache_path_is_state_cache_source_hash_json() {
        assert_eq!(
            cache_path(Path::new("/state"), "github", "abc123"),
            Path::new("/state/cache/github/abc123.json")
        );
    }

    #[test]
    fn the_core_window_converts_field_for_field() {
        let core = autostand_core::dates::compute_window(day(3));
        let adapted = to_date_window(&core);
        assert_eq!(adapted.start, core.range_start);
        assert_eq!(adapted.end, core.range_end);
        assert_eq!(adapted.dates, core.dates);
    }

    #[test]
    fn window_hash_is_stable_and_scoped_to_the_range() {
        let window = test_window();
        assert_eq!(window_hash(&window), window_hash(&window));

        let other = DateWindow {
            start: day(1),
            end: day(3),
            dates: vec![day(1), day(2), day(3)],
        };
        assert_ne!(window_hash(&window), window_hash(&other));
    }

    #[test]
    fn a_fresh_entry_reads_back_verbatim() {
        let dir = temp_dir("fresh");
        let key = window_hash(&test_window());
        let data = SourceData {
            facts: Some("### repo: autostand".into()),
            notes: None,
            enrichment: Some("## CONTEXT".into()),
            files: vec!["autostand/lib.rs".into()],
        };
        write_cache(&dir, "local-git", &key, &data).expect("write cache");

        let read = read_cache(&dir, "local-git", &key).expect("cache hit");
        assert_eq!(read.facts, data.facts);
        assert_eq!(read.enrichment, data.enrichment);
        assert_eq!(read.files, data.files);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_entries_are_scoped_per_source_and_per_window() {
        let dir = temp_dir("scope");
        let key = window_hash(&test_window());
        write_cache(&dir, "local-git", &key, &data_with_facts("f")).expect("write cache");

        assert!(read_cache(&dir, "github", &key).is_none());
        assert!(read_cache(&dir, "local-git", "other-window").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_entry_is_a_miss_not_an_error() {
        let dir = temp_dir("missing");
        assert!(read_cache(&dir, "github", "nope").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_expired_entry_is_ignored() {
        let dir = temp_dir("expired");
        let key = window_hash(&test_window());
        let stale = Utc::now()
            - ChronoDuration::seconds(
                i64::try_from(CACHE_TTL.as_secs()).expect("ttl fits in i64") + 60,
            );
        write_cache_at(&dir, "github", &key, &data_with_facts("stale"), stale).expect("write");

        assert!(
            cache_path(&dir, "github", &key).exists(),
            "entry was written"
        );
        assert!(
            read_cache(&dir, "github", &key).is_none(),
            "TTL not enforced"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_entry_just_inside_the_ttl_still_hits() {
        let dir = temp_dir("edge");
        let key = window_hash(&test_window());
        let recent = Utc::now()
            - ChronoDuration::seconds(
                i64::try_from(CACHE_TTL.as_secs()).expect("ttl fits in i64") - 60,
            );
        write_cache_at(&dir, "github", &key, &data_with_facts("fresh"), recent).expect("write");

        assert!(read_cache(&dir, "github", &key).is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_future_timestamp_counts_as_stale() {
        let now = Utc::now();
        assert!(!is_fresh(now + ChronoDuration::seconds(120), now));
        assert!(is_fresh(now, now));
    }

    #[test]
    fn corrupt_cache_json_is_ignored_rather_than_fatal() {
        let dir = temp_dir("corrupt");
        let path = cache_path(&dir, "codex", "deadbeef");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, "{ not json at all").expect("write");

        assert!(read_cache(&dir, "codex", "deadbeef").is_none());

        // A well-formed JSON document of the wrong shape is a miss too.
        std::fs::write(&path, r#"{"cached_at":"2026-08-03T00:00:00Z"}"#).expect("write");
        assert!(read_cache(&dir, "codex", "deadbeef").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writing_an_entry_twice_replaces_it_and_leaves_no_temp_file() {
        let dir = temp_dir("replace");
        let key = window_hash(&test_window());
        write_cache(&dir, "opencode", &key, &data_with_facts("v1")).expect("write 1");
        write_cache(&dir, "opencode", &key, &data_with_facts("v2")).expect("write 2");

        assert_eq!(
            read_cache(&dir, "opencode", &key).and_then(|d| d.facts),
            Some("v2".to_string())
        );
        let entries: Vec<_> = std::fs::read_dir(dir.join("cache").join("opencode"))
            .expect("read_dir")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(entries.len(), 1, "unexpected leftovers: {entries:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_cache_removes_everything_and_tolerates_a_missing_dir() {
        let dir = temp_dir("clear");
        let key = window_hash(&test_window());
        write_cache(&dir, "github", &key, &data_with_facts("x")).expect("write");
        clear_cache(&dir).expect("clear");
        assert!(read_cache(&dir, "github", &key).is_none());
        clear_cache(&dir).expect("clearing an absent cache is a no-op");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn cached_entries_are_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("mode");
        let key = window_hash(&test_window());
        write_cache(&dir, "claude-code", &key, &data_with_facts("secret-ish")).expect("write");

        let mode = std::fs::metadata(cache_path(&dir, "claude-code", &key))
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
