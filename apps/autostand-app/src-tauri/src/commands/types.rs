//! Serde DTOs mirroring the TypeScript interfaces in
//! `docs/tauri/02-ipc-contracts.md`.
//!
//! Field names are `snake_case` on both sides (the TS contract uses `snake_case`
//! too — see the interfaces in `02-ipc-contracts.md`), so structs need no
//! `#[serde(rename_all)]`.
//!
//! Enums are a different story: the contract fixes each variant's *wire* string
//! and those strings are not uniform. `RenderMode` / `ProviderMode` are spelled
//! `PascalCase` (`"Auto"`, `"CliFirst"`), the status-ish enums are `snake_case`
//! (`"ok"`, `"llm_fallback"`, `"keychain"`) and the scheduler enums are
//! `kebab-case` (`"task-scheduler"`, `"self-heal"`). Every enum below therefore
//! carries an explicit `#[serde(rename_all = …)]` — the default (variant name
//! verbatim) is only correct for the two `PascalCase` ones, and relying on it
//! elsewhere silently breaks the frontend.

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
///
/// Wire values are `PascalCase` (`"Auto" | "Llm" | "Det"`) per the contract, so
/// the serde default (variant name verbatim) is what we want here.
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
///
/// Wire values are `PascalCase` (`"CliFirst" | "ApiFallback" | "CliOnly" |
/// "ApiOnly"`) per the contract — serde default applies.
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
// The 8 booleans *are* the contract (`DataSourceConfigs` in
// `02-ipc-contracts.md`); collapsing them into a set/bitflags would change the
// JSON the frontend reads, so the lint does not apply here.
#[allow(clippy::struct_excessive_bools)]
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
///
/// Wire values: `"keychain" | "env" | "none"`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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
///
/// Wire values: `"ok" | "skip" | "error"`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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
///
/// Wire values: `"llm" | "det" | "llm_fallback"` — the same strings
/// `autostand-core` writes into the audit sidecar.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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
    /// `OpenCode` session ids.
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
    /// Render mode preference recorded at render time.
    pub render_mode: AuditRenderMode,
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

/// Render mode as recorded in the audit sidecar.
///
/// Distinct from [`RenderMode`] on purpose: the config field is `PascalCase` on
/// the wire (`"Auto"`), while `AuditData.render_mode` is lowercase
/// (`"auto" | "llm" | "det"`) because `autostand-core` writes it that way.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditRenderMode {
    /// Try LLM, fall back to deterministic.
    #[default]
    Auto,
    /// Always LLM.
    Llm,
    /// Always deterministic.
    Det,
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
    /// `OpenCode` session ids.
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
///
/// Wire values: `"launchd" | "systemd" | "task-scheduler" | "in-process" |
/// "none"`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SchedulerSource {
    /// macOS launchd.
    Launchd,
    /// Linux systemd.
    Systemd,
    /// Windows Task Scheduler.
    TaskScheduler,
    /// In-process (dev / fallback).
    #[default]
    InProcess,
    /// No scheduler installed.
    None,
}

/// Last trigger source.
///
/// Wire values: `"scheduled" | "manual" | "self-heal"`. Also used as the
/// `trigger` field of the `pipeline-started` event payload.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LastTrigger {
    /// Cron-scheduled run.
    Scheduled,
    /// Manual `trigger_run_now`.
    #[default]
    Manual,
    /// Self-heal of a missed run.
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
// The `_dir` suffix on every field is the contract (`SettingsPaths` in
// `02-ipc-contracts.md`) and is what `validate_paths` echoes back as the
// per-path `label`; renaming the fields would break both.
#[allow(clippy::struct_field_names)]
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
#[cfg(test)]
mod tests {
    use super::{
        ApiKeyMode, ApiKeyStatus, AppConfig, AuditData, AuditRenderMode, AuditSidecar,
        CompileResult, CompileStatus, LastTrigger, ProviderMode, RenderMode, RenderUsed,
        SchedulerSource, SchedulerStatus,
    };
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::json;
    use std::fmt::Debug;

    /// Assert a variant's exact wire string and that it survives a round-trip.
    ///
    /// The round-trip half matters as much as the string: `serde` derives the
    /// reader and the writer from the same attribute, so a contract test that
    /// only deserialized would pass even with the wrong `rename_all`.
    fn assert_wire<T>(value: &T, expected: &str)
    where
        T: Serialize + DeserializeOwned + PartialEq + Debug,
    {
        let json = serde_json::to_string(value).expect("serializes");
        assert_eq!(json, format!("\"{expected}\""));
        let back: T = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(&back, value);
    }

    #[test]
    fn compile_status_wire_values() {
        assert_wire(&CompileStatus::Ok, "ok");
        assert_wire(&CompileStatus::Skip, "skip");
        assert_wire(&CompileStatus::Error, "error");
    }

    #[test]
    fn render_used_wire_values() {
        assert_wire(&RenderUsed::Llm, "llm");
        assert_wire(&RenderUsed::Det, "det");
        assert_wire(&RenderUsed::LlmFallback, "llm_fallback");
    }

    #[test]
    fn api_key_mode_wire_values() {
        assert_wire(&ApiKeyMode::Keychain, "keychain");
        assert_wire(&ApiKeyMode::Env, "env");
        assert_wire(&ApiKeyMode::None, "none");
    }

    #[test]
    fn scheduler_source_wire_values() {
        assert_wire(&SchedulerSource::Launchd, "launchd");
        assert_wire(&SchedulerSource::Systemd, "systemd");
        assert_wire(&SchedulerSource::TaskScheduler, "task-scheduler");
        assert_wire(&SchedulerSource::InProcess, "in-process");
        assert_wire(&SchedulerSource::None, "none");
    }

    #[test]
    fn last_trigger_wire_values() {
        assert_wire(&LastTrigger::Scheduled, "scheduled");
        assert_wire(&LastTrigger::Manual, "manual");
        assert_wire(&LastTrigger::SelfHeal, "self-heal");
    }

    #[test]
    fn audit_render_mode_wire_values_are_lowercase() {
        assert_wire(&AuditRenderMode::Auto, "auto");
        assert_wire(&AuditRenderMode::Llm, "llm");
        assert_wire(&AuditRenderMode::Det, "det");
    }

    #[test]
    fn config_render_mode_stays_pascal_case() {
        // `AppConfig.render_mode` and `AuditData.render_mode` intentionally
        // disagree: the contract spells the former "Auto" and the latter "auto".
        assert_wire(&RenderMode::Auto, "Auto");
        assert_wire(&RenderMode::Llm, "Llm");
        assert_wire(&RenderMode::Det, "Det");
    }

    #[test]
    fn provider_mode_stays_pascal_case() {
        assert_wire(&ProviderMode::CliFirst, "CliFirst");
        assert_wire(&ProviderMode::ApiFallback, "ApiFallback");
        assert_wire(&ProviderMode::CliOnly, "CliOnly");
        assert_wire(&ProviderMode::ApiOnly, "ApiOnly");
    }

    #[test]
    fn no_enum_leaks_a_rust_variant_name() {
        // Regression guard for the whole family: every one of these used to
        // serialize as its Rust identifier.
        let leaked = [
            serde_json::to_string(&CompileStatus::Ok).unwrap(),
            serde_json::to_string(&RenderUsed::LlmFallback).unwrap(),
            serde_json::to_string(&ApiKeyMode::Keychain).unwrap(),
            serde_json::to_string(&SchedulerSource::TaskScheduler).unwrap(),
            serde_json::to_string(&LastTrigger::SelfHeal).unwrap(),
            serde_json::to_string(&AuditRenderMode::Auto).unwrap(),
        ];
        for value in leaked {
            assert!(
                !value.chars().any(|c| c.is_ascii_uppercase()),
                "{value} still carries a PascalCase variant name"
            );
        }
    }

    #[test]
    fn compile_result_serializes_the_documented_shape() {
        let value = serde_json::to_value(CompileResult {
            date: "2026-08-03".to_string(),
            host: "MacStudio-de-Miguel".to_string(),
            status: CompileStatus::Ok,
            render_used: RenderUsed::LlmFallback,
            fellback: true,
            audit_path: Some("/state/audit/2026-08-03-MacStudio-de-Miguel.json".to_string()),
            file_path: "/dailies/2026-08-03.md".to_string(),
            accumulated_count: 2,
            message: "ok".to_string(),
        })
        .unwrap();
        assert_eq!(value["status"], json!("ok"));
        assert_eq!(value["render_used"], json!("llm_fallback"));
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
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
    }

    #[test]
    fn scheduler_status_serializes_the_documented_shape() {
        let value = serde_json::to_value(SchedulerStatus {
            enabled: true,
            source: SchedulerSource::TaskScheduler,
            cron: "0 7-19 * * 1-5".to_string(),
            next_run_at: Some("2026-08-03T07:00:00Z".to_string()),
            last_run_at: None,
            last_trigger: Some(LastTrigger::SelfHeal),
        })
        .unwrap();
        assert_eq!(value["source"], json!("task-scheduler"));
        assert_eq!(value["last_trigger"], json!("self-heal"));
        assert_eq!(value["last_run_at"], json!(null));
    }

    #[test]
    fn api_key_status_serializes_set_plus_mode() {
        let value = serde_json::to_value(ApiKeyStatus {
            set: true,
            mode: ApiKeyMode::Keychain,
        })
        .unwrap();
        assert_eq!(value, json!({ "set": true, "mode": "keychain" }));
    }

    #[test]
    fn audit_sidecar_serializes_render_used_lowercase() {
        let value = serde_json::to_value(AuditSidecar {
            path: "/state/audit/2026-08-03-host.json".to_string(),
            date: "2026-08-03".to_string(),
            host: "host".to_string(),
            rendered_at: "2026-08-03T07:00:00Z".to_string(),
            render_used: RenderUsed::Llm,
        })
        .unwrap();
        assert_eq!(value["render_used"], json!("llm"));
    }

    #[test]
    fn audit_data_render_mode_is_lowercase_unlike_the_config_field() {
        let audit = serde_json::to_value(AuditData::default()).unwrap();
        let config = serde_json::to_value(AppConfig::default()).unwrap();
        assert_eq!(audit["render_mode"], json!("auto"));
        assert_eq!(config["render_mode"], json!("Auto"));
    }

    #[test]
    fn app_config_defaults_serialize_the_documented_keys() {
        let value = serde_json::to_value(AppConfig::default()).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "dailies_dir",
                "data_sources",
                "git_refs",
                "github_dir",
                "host_slug_override",
                "jira_base",
                "llm",
                "render_mode",
                "review",
                "scheduler",
                "scrub",
                "standup_authors",
            ]
        );
    }

    #[test]
    fn defaults_are_the_documented_ones() {
        assert_eq!(CompileStatus::default(), CompileStatus::Ok);
        assert_eq!(RenderUsed::default(), RenderUsed::Det);
        assert_eq!(ApiKeyMode::default(), ApiKeyMode::None);
        assert_eq!(SchedulerSource::default(), SchedulerSource::InProcess);
        assert_eq!(RenderMode::default(), RenderMode::Auto);
        assert_eq!(ProviderMode::default(), ProviderMode::CliFirst);
        assert_eq!(AuditRenderMode::default(), AuditRenderMode::Auto);
    }
}
