//! Serde DTOs mirroring the TypeScript interfaces in
//! `docs/tauri/02-ipc-contracts.md`.
//!
//! Field names are `snake_case` on both sides (the TS contract uses snake_case
//! too — see the interfaces in `02-ipc-contracts.md`), so no `#[serde(rename_all)]`
//! is needed.

use serde::{Deserialize, Serialize};

// ── AppConfig + nested config ─────────────────────────────────────────────

/// Full app configuration persisted to the Tauri Store.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// Directory containing git repos scanned by `local-git`.
    pub github_dir: String,
    /// Directory where `dailies/<date>.md` files are written.
    pub dailies_dir: String,
    /// Authors filter for `local-git` (comma-separated git author names).
    pub standup_authors: Vec<String>,
    /// Git refs to scan (e.g. `--all`).
    pub git_refs: String,
    /// Jira base URL (e.g. `https://org.atlassian.net/browse`).
    pub jira_base: String,
    /// Manual host slug override; `null` ⇒ auto-detect.
    pub host_slug_override: Option<String>,
    /// Render mode preference.
    pub render_mode: RenderMode,
    /// LLM provider configuration block.
    pub llm: LlmConfig,
    /// Per-data-source enablement flags.
    pub data_sources: DataSourceConfigs,
    /// Scheduler configuration block.
    pub scheduler: SchedulerConfig,
    /// PR review configuration block.
    pub review: ReviewConfig,
    /// Secret-scrub configuration block.
    pub scrub: ScrubConfig,
}

/// Render mode preference.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum RenderMode {
    /// Try LLM, fall back to deterministic.
    #[default]
    Auto,
    /// Always LLM (error if no provider available).
    Llm,
    /// Always deterministic (no LLM).
    Det,
}

/// LLM configuration block.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    /// Preferred provider id (`claude` | `ollama` | `openai` | `gemini` | `grok`).
    pub preferred_provider: String,
    /// Per-provider configuration.
    pub providers: Vec<ProviderConfig>,
}

/// Per-provider configuration stored in `AppConfig.llm.providers`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    /// Stable provider id.
    pub id: String,
    /// Whether this provider is enabled.
    pub enabled: bool,
    /// Access mode.
    pub mode: ProviderMode,
    /// Model name (provider-specific).
    pub model: String,
    /// Override path to the CLI binary, or `null` for PATH lookup.
    pub cli_path: Option<String>,
    /// Keychain reference for the API key (never the key itself), or `null`.
    pub api_key_ref: Option<String>,
    /// Override API base URL, or `null`.
    pub api_base_url: Option<String>,
    /// Timeout for a render call, in seconds.
    pub timeout_secs: u64,
}

/// Provider access mode.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderMode {
    /// Try CLI first, fall back to API.
    #[default]
    CliFirst,
    /// API only when CLI unavailable.
    ApiFallback,
    /// CLI only (no API).
    CliOnly,
    /// API only (no CLI).
    ApiOnly,
}

/// Per-data-source enablement flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceConfigs {
    /// `local-git` (always on in practice; forced elsewhere).
    pub local_git: bool,
    /// `github` via `gh` CLI.
    pub github: bool,
    /// `claude-code` sessions.
    pub claude_code: bool,
    /// `remember-plugin` narrative notes.
    pub remember: bool,
    /// `opencode` SQLite/JSON.
    pub opencode: bool,
    /// `codex` CLI sessions.
    pub codex: bool,
    /// `gemini-cli` sessions.
    pub gemini_cli: bool,
    /// `grok-cli` sessions.
    pub grok_cli: bool,
}

impl Default for DataSourceConfigs {
    fn default() -> Self {
        Self {
            local_git: true,
            github: true,
            claude_code: true,
            remember: true,
            opencode: true,
            codex: false,
            gemini_cli: false,
            grok_cli: false,
        }
    }
}

/// Scheduler configuration block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Whether the in-process/system scheduler is enabled.
    pub enabled: bool,
    /// 5-field POSIX cron expression.
    pub cron: String,
    /// Self-heal missed runs.
    pub self_heal: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cron: "0 7-19 * * 1-5".to_string(),
            self_heal: true,
        }
    }
}

/// PR review configuration block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewConfig {
    /// Reviewer login to filter on.
    pub reviewer: String,
    /// Organization to scope PR discovery.
    pub pr_org: String,
    /// Max PRs to fetch.
    pub max_prs: u32,
    /// Max comment length to include.
    pub comment_len: u32,
    /// Include self-reviews.
    pub include_self_reviews: bool,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            reviewer: String::new(),
            pr_org: String::new(),
            max_prs: 20,
            comment_len: 280,
            include_self_reviews: false,
        }
    }
}

/// Secret-scrub configuration block.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScrubConfig {
    /// Apply alias-scrub (host-alias redaction).
    pub alias_scrub: bool,
    /// Minimum length for alias-scrub to kick in.
    pub alias_scrub_min: u32,
    /// Extra meta to strip, or `null`.
    pub meta_extra: Option<String>,
}

// ── Data sources list ────────────────────────────────────────────────────

/// Data source descriptor returned by `list_data_sources`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceConfig {
    /// Stable source id (e.g. `local-git`).
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Whether this source is enabled.
    pub enabled: bool,
    /// Short description (from `docs/data-sources/00-sources-overview.md`).
    pub description: String,
}

// ── LLM providers ─────────────────────────────────────────────────────────

/// LLM provider descriptor returned by `list_llm_providers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    /// Stable provider id.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Whether this provider is enabled.
    pub enabled: bool,
    /// Access mode.
    pub mode: ProviderMode,
    /// Configured model name (may be empty).
    pub model: String,
    /// Detected CLI info.
    pub cli: CliDetection,
    /// API key presence + source.
    pub api_key: ApiKeyStatus,
}

/// CLI binary detection result.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CliDetection {
    /// Whether the binary was found on PATH.
    pub found: bool,
    /// Absolute path to the binary (empty if not found).
    pub path: String,
    /// `--version` output (empty if not found).
    pub version: String,
}

/// API key presence + where it came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ApiKeyMode {
    /// Stored in the OS keychain.
    Keychain,
    /// Set via environment variable.
    Env,
    /// No key set.
    #[default]
    None,
}

/// API key status returned by `get_api_key_status`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiKeyStatus {
    /// Whether a key is available.
    pub set: bool,
    /// Where the key came from.
    pub mode: ApiKeyMode,
}

/// Result of `test_llm_provider`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestProviderResult {
    /// Whether the test succeeded.
    pub ok: bool,
    /// Human-readable result message.
    pub message: String,
    /// Round-trip latency in milliseconds.
    pub latency_ms: u64,
}

// ── Compile + pipeline ───────────────────────────────────────────────────

/// Result of a compile run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompileResult {
    /// Filing date (`YYYY-MM-DD`).
    pub date: String,
    /// Host slug.
    pub host: String,
    /// `ok` | `skip` | `error`.
    pub status: CompileStatus,
    /// Which renderer produced the body.
    pub render_used: RenderUsed,
    /// True if Auto mode fell back to deterministic.
    pub fellback: bool,
    /// Path to the audit sidecar JSON, if written.
    pub audit_path: Option<String>,
    /// Path to the written `dailies/<date>.md` file.
    pub file_path: String,
    /// Number of PREV bullets re-injected by accumulate.
    pub accumulated_count: u32,
    /// Human-readable message.
    pub message: String,
}

/// Compile outcome.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompileStatus {
    /// Successful compile.
    #[default]
    Ok,
    /// Skipped (already up to date / no work).
    Skip,
    /// Error.
    Error,
}

/// Which renderer produced the body.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum RenderUsed {
    /// LLM renderer.
    Llm,
    /// Deterministic renderer.
    #[default]
    Det,
    /// LLM renderer that fell back to deterministic after validation failure.
    LlmFallback,
}

/// Standup file content returned by `read_standup_file`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StandupFileContent {
    /// Filing date (`YYYY-MM-DD`).
    pub date: String,
    /// Human-readable title (e.g. `Daily Standup — August 03, 2026`).
    pub title: String,
    /// Italic subtitle (e.g. `_Work completed August 01–02, 2026._`).
    pub subtitle: String,
    /// One AUTO block per host slug.
    pub auto_blocks: Vec<AutoBlockDto>,
    /// Verbatim inner content of the MANUAL block.
    pub manual_region: String,
}

/// AUTO block DTO.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutoBlockDto {
    /// Host slug.
    pub host: String,
    /// Verbatim inner content of the AUTO block.
    pub body: String,
}

/// Audit sidecar descriptor returned by `list_audit_sidecars`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuditSidecar {
    /// Absolute path to the sidecar JSON.
    pub path: String,
    /// Filing date (`YYYY-MM-DD`).
    pub date: String,
    /// Host slug.
    pub host: String,
    /// ISO-8601 UTC render timestamp.
    pub rendered_at: String,
    /// Which renderer was used.
    pub render_used: RenderUsed,
}

/// Audit data returned by `read_audit_sidecar`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuditData {
    /// Filing date (`YYYY-MM-DD`).
    pub file: String,
    /// Host slug.
    pub host: String,
    /// ISO-8601 UTC render timestamp.
    pub rendered_at: String,
    /// Window of work covered.
    pub window: DateRangeDto,
    /// Repo facts.
    pub facts: Vec<RepoFacts>,
    /// Narrative notes.
    pub notes: Vec<NoteRef>,
    /// GitHub activity string (raw).
    pub github: Option<String>,
    /// Claude Code conversation digest.
    pub conv: Option<String>,
    /// PR review enrichment.
    pub prrev: Option<String>,
    /// Files edited in Claude Code sessions.
    pub claude_files: Vec<String>,
    /// OpenCode session ids.
    pub opencode_sessions: Vec<String>,
    /// Codex session ids.
    pub codex_sessions: Vec<String>,
    /// Gemini CLI session ids.
    pub gemini_sessions: Vec<String>,
    /// Grok CLI session ids.
    pub grok_sessions: Vec<String>,
    /// Forbidden (cross-day) tickets.
    pub forbidden_tickets: Vec<String>,
    /// Covered tickets (already in git/github).
    pub covered_tickets: Vec<String>,
    /// SKEW records.
    pub skew: Vec<SkewRecord>,
    /// Ticket → commit days map.
    pub ticket_days: std::collections::BTreeMap<String, Vec<String>>,
    /// Render mode preference.
    pub render_mode: String,
    /// Which renderer actually produced the body.
    pub render_used: RenderUsed,
    /// Provider id used, if any.
    pub provider: Option<String>,
    /// Model id used, if any.
    pub model: Option<String>,
    /// Whether the run fell back to deterministic.
    pub fellback: bool,
    /// Audit hash.
    pub hash: String,
    /// Number of PREV bullets re-injected.
    pub accumulated_count: u32,
}

/// Date range DTO used in audit + gather preview.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DateRangeDto {
    /// Inclusive start (`YYYY-MM-DD`).
    pub range_start: String,
    /// Inclusive end (`YYYY-MM-DD`).
    pub range_end: String,
}

/// Repo facts DTO.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoFacts {
    /// Repo name (basename of the path).
    pub repo: String,
    /// Ticket key, if any.
    pub ticket: Option<String>,
    /// Commit/title subject.
    pub title: String,
    /// Commits in the window.
    pub commits: Vec<CommitDto>,
}

/// Commit DTO.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommitDto {
    /// Commit SHA.
    pub sha: String,
    /// Commit subject.
    pub subject: String,
    /// ISO-8601 commit date.
    pub date: String,
    /// Files changed.
    pub files: Vec<String>,
}

/// Narrative note reference.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NoteRef {
    /// Path to the note file.
    pub source: String,
    /// Note date (`YYYY-MM-DD`).
    pub date: String,
    /// Note clauses.
    pub clauses: Vec<String>,
}

/// SKEW record DTO (ticket vs note date skew).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkewRecord {
    /// Ticket key.
    pub ticket: String,
    /// Note date (`YYYY-MM-DD`).
    pub note_date: String,
    /// Commit days for the ticket.
    pub commit_days: Vec<String>,
}

/// Gather preview returned by `preview_gather`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatherPreview {
    /// Filing date (`YYYY-MM-DD`).
    pub date: String,
    /// Host slug.
    pub host: String,
    /// Window of work covered.
    pub window: DateRangeDto,
    /// Repo facts.
    pub facts: Vec<RepoFacts>,
    /// Narrative notes.
    pub notes: Vec<NoteRef>,
    /// GitHub activity string (raw).
    pub github: Option<String>,
    /// Claude Code conversation digest.
    pub conv: Option<String>,
    /// PR review enrichment.
    pub prrev: Option<String>,
    /// Files edited in Claude Code sessions.
    pub claude_files: Vec<String>,
    /// OpenCode session ids.
    pub opencode_sessions: Vec<String>,
    /// Codex session ids.
    pub codex_sessions: Vec<String>,
    /// Gemini CLI session ids.
    pub gemini_sessions: Vec<String>,
    /// Grok CLI session ids.
    pub grok_sessions: Vec<String>,
    /// Forbidden (cross-day) tickets.
    pub forbidden_tickets: Vec<String>,
    /// Covered tickets (already in git/github).
    pub covered_tickets: Vec<String>,
    /// SKEW records.
    pub skew: Vec<SkewRecord>,
}

/// Scheduler status returned by `get_scheduler_status`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchedulerStatus {
    /// Whether the scheduler is enabled.
    pub enabled: bool,
    /// Where the schedule comes from.
    pub source: SchedulerSource,
    /// 5-field POSIX cron expression.
    pub cron: String,
    /// ISO-8601 next-run timestamp.
    pub next_run_at: Option<String>,
    /// ISO-8601 last-run timestamp.
    pub last_run_at: Option<String>,
    /// Last trigger source.
    pub last_trigger: Option<LastTrigger>,
}

/// Where the schedule comes from.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum SchedulerSource {
    /// macOS launchd.
    Launchd,
    /// Linux systemd.
    Systemd,
    /// Windows Task Scheduler.
    #[allow(non_camel_case_types)]
    TaskScheduler,
    /// In-process (dev / fallback).
    #[default]
    InProcess,
    /// No scheduler installed.
    None,
}

/// Last trigger source.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LastTrigger {
    /// Cron-scheduled run.
    Scheduled,
    /// Manual `trigger_run_now`.
    Manual,
    /// Self-heal of a missed run.
    #[allow(non_camel_case_types)]
    SelfHeal,
}

/// Repo info returned by `discover_repos`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoInfo {
    /// Absolute path to the repo.
    pub path: String,
    /// Repo name (basename).
    pub name: String,
    /// `remote.origin.url`, if configured.
    pub remote: Option<String>,
    /// ISO-8601 last-commit timestamp.
    pub last_commit_at: Option<String>,
}

/// All configured paths returned by `get_settings_paths`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsPaths {
    /// `GITHUB_DIR` (from config).
    pub github_dir: String,
    /// Dailies output dir.
    pub dailies_dir: String,
    /// `~/.claude`.
    pub claude_dir: String,
    /// `~/.codex`.
    pub codex_dir: String,
    /// `~/.gemini`.
    pub gemini_dir: String,
    /// `~/.local/share/opencode` or `~/.config/opencode`.
    pub opencode_dir: String,
    /// autostand state dir.
    pub state_dir: String,
    /// autostand config dir.
    pub config_dir: String,
    /// Audit sidecar dir (`state/audit`).
    pub audit_dir: String,
}

/// Payload for the `pipeline-progress` event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineProgress {
    /// Filing date (`YYYY-MM-DD`).
    pub date: String,
    /// Host slug.
    pub host: String,
    /// Human-readable step name (e.g. `gather`, `render_llm`).
    pub step: String,
    /// Progress percent `0..=100`.
    pub percent: u8,
}

/// Per-path validation result returned by `validate_paths`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathValidation {
    /// Absolute path.
    pub path: String,
    /// Label (e.g. `github_dir`).
    pub label: String,
    /// Whether the path exists.
    pub exists: bool,
    /// Whether the path is readable.
    pub readable: bool,
    /// Failure message, if any.
    pub message: Option<String>,
}