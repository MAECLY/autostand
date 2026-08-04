/**
 * TypeScript mirrors of the Rust serde DTOs crossing the Tauri IPC boundary.
 *
 * Field names are snake_case because the Rust structs in
 * `src-tauri/src/commands/types.rs` and `src-tauri/src/state.rs` carry no
 * `#[serde(rename_all)]` attribute. Contract: `docs/tauri/02-ipc-contracts.md`.
 */

// ── Enumerations ──────────────────────────────────────────────────────────

/** `AppConfig.render_mode` — stays PascalCase in Rust. */
export type RenderMode = "Auto" | "Llm" | "Det";

/** `ProviderConfig.mode` — stays PascalCase in Rust. */
export type ProviderMode = "CliFirst" | "ApiFallback" | "CliOnly" | "ApiOnly";

/** Outcome of a compile run. */
export type CompileStatus = "ok" | "skip" | "error";

/** Which renderer produced the standup body. */
export type RenderUsed = "llm" | "det" | "llm_fallback";

/** Where an API key came from. */
export type ApiKeyMode = "keychain" | "env" | "none";

/** Where the cron schedule is installed. */
export type SchedulerSource =
  | "launchd"
  | "systemd"
  | "task-scheduler"
  | "in-process"
  | "none";

/** What kicked off a run. */
export type TriggerSource = "scheduled" | "manual" | "self-heal";

/** Coarse pipeline state machine position. */
export type PipelineState =
  | "idle"
  | "gathering"
  | "rendering"
  | "done"
  | "error";

/** `AuditData.render_mode` — lowercase in the sidecar JSON, unlike `RenderMode`. */
export type AuditRenderMode = "auto" | "llm" | "det";

/** Transport probed by `test_llm_provider`. */
export type ProviderTestMode = "cli" | "api";

// ── App configuration ─────────────────────────────────────────────────────

export interface AppConfig {
  github_dir: string;
  dailies_dir: string;
  standup_authors: string[];
  git_refs: string;
  jira_base: string;
  host_slug_override: string | null;
  render_mode: RenderMode;
  llm: LlmConfig;
  data_sources: DataSourceConfigs;
  scheduler: SchedulerConfig;
  review: ReviewConfig;
  scrub: ScrubConfig;
}

export interface LlmConfig {
  preferred_provider: string;
  providers: ProviderConfig[];
}

export interface ProviderConfig {
  /** `claude` | `ollama` | `openai` | `gemini` | `grok`. */
  id: string;
  enabled: boolean;
  mode: ProviderMode;
  model: string;
  cli_path: string | null;
  /** Keychain reference — never the key itself. */
  api_key_ref: string | null;
  api_base_url: string | null;
  timeout_secs: number;
}

export interface DataSourceConfigs {
  local_git: boolean;
  github: boolean;
  claude_code: boolean;
  remember: boolean;
  opencode: boolean;
  codex: boolean;
  gemini_cli: boolean;
  grok_cli: boolean;
}

export interface SchedulerConfig {
  enabled: boolean;
  /** 5-field POSIX cron expression. */
  cron: string;
  self_heal: boolean;
}

export interface ReviewConfig {
  reviewer: string;
  pr_org: string;
  max_prs: number;
  comment_len: number;
  include_self_reviews: boolean;
}

export interface ScrubConfig {
  alias_scrub: boolean;
  alias_scrub_min: number;
  meta_extra: string | null;
}

// ── Data sources ──────────────────────────────────────────────────────────

export interface DataSourceConfig {
  /** Stable source id, e.g. `local-git`. */
  id: string;
  label: string;
  enabled: boolean;
  description: string;
}

// ── LLM providers ─────────────────────────────────────────────────────────

export interface LlmProviderConfig {
  id: string;
  label: string;
  enabled: boolean;
  mode: ProviderMode;
  model: string;
  cli: CliDetection;
  api_key: ApiKeyStatus;
}

export interface CliDetection {
  found: boolean;
  /** Absolute path to the binary; empty when not found. */
  path: string;
  /** `--version` output; empty when not found. */
  version: string;
}

export interface ApiKeyStatus {
  set: boolean;
  mode: ApiKeyMode;
}

export interface TestProviderResult {
  ok: boolean;
  message: string;
  latency_ms: number;
}

// ── Compile + pipeline ────────────────────────────────────────────────────

export interface CompileResult {
  /** Filing date, `YYYY-MM-DD`. */
  date: string;
  host: string;
  status: CompileStatus;
  render_used: RenderUsed;
  fellback: boolean;
  audit_path: string | null;
  file_path: string;
  /** Bullets re-injected from PREV by accumulate. */
  accumulated_count: number;
  message: string;
}

export interface StandupFileContent {
  date: string;
  /** e.g. `Daily Standup — August 03, 2026`. */
  title: string;
  /** e.g. `_Work completed August 01–02, 2026._`. */
  subtitle: string;
  /** One entry per host slug. */
  auto_blocks: AutoBlock[];
  /** Verbatim inner content of the MANUAL block. */
  manual_region: string;
}

export interface AutoBlock {
  host: string;
  /** Verbatim inner content of the AUTO block. */
  body: string;
}

export interface PipelineStatus {
  state: PipelineState;
  current_date: string | null;
  current_host: string | null;
  /** Human-readable step name, e.g. `gather`, `render_llm`. */
  step: string | null;
  /** 0..=100. */
  percent: number;
  /** ISO-8601. */
  last_run_at: string | null;
  last_result: CompileResult | null;
  error: string | null;
}

// ── Audit ─────────────────────────────────────────────────────────────────

export interface AuditSidecar {
  path: string;
  date: string;
  host: string;
  /** ISO-8601 UTC. */
  rendered_at: string;
  render_used: RenderUsed;
}

export interface AuditData {
  file: string;
  host: string;
  /** ISO-8601 UTC. */
  rendered_at: string;
  window: DateRange;
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
  /** Ticket key → commit days. */
  ticket_days: Record<string, string[]>;
  render_mode: AuditRenderMode;
  render_used: RenderUsed;
  provider: string | null;
  model: string | null;
  fellback: boolean;
  hash: string;
  accumulated_count: number;
}

export interface DateRange {
  /** Inclusive, `YYYY-MM-DD`. */
  range_start: string;
  /** Inclusive, `YYYY-MM-DD`. */
  range_end: string;
}

export interface RepoFacts {
  repo: string;
  ticket: string | null;
  title: string;
  commits: CommitInfo[];
}

export interface CommitInfo {
  sha: string;
  subject: string;
  /** ISO-8601. */
  date: string;
  files: string[];
}

export interface NoteRef {
  /** Path to the note file. */
  source: string;
  date: string;
  clauses: string[];
}

export interface SkewRecord {
  ticket: string;
  note_date: string;
  commit_days: string[];
}

export interface GatherPreview {
  date: string;
  host: string;
  window: DateRange;
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
}

// ── Scheduler ─────────────────────────────────────────────────────────────

export interface SchedulerStatus {
  enabled: boolean;
  source: SchedulerSource;
  cron: string;
  /** ISO-8601. */
  next_run_at: string | null;
  /** ISO-8601. */
  last_run_at: string | null;
  last_trigger: TriggerSource | null;
}

// ── Repos + settings ──────────────────────────────────────────────────────

export interface RepoInfo {
  path: string;
  name: string;
  remote: string | null;
  /** ISO-8601. */
  last_commit_at: string | null;
}

export interface SettingsPaths {
  github_dir: string;
  dailies_dir: string;
  claude_dir: string;
  codex_dir: string;
  gemini_dir: string;
  opencode_dir: string;
  state_dir: string;
  config_dir: string;
  audit_dir: string;
}

export interface PathValidation {
  path: string;
  /** Config key the path came from, e.g. `github_dir`. */
  label: string;
  exists: boolean;
  readable: boolean;
  message: string | null;
}

// ── Errors ────────────────────────────────────────────────────────────────

/**
 * Serialized `AppError`. `code` is the Rust variant tag; it is left open
 * because the backend may add variants ahead of this file.
 */
export interface AppError {
  code: string;
  message: string;
}

// ── Event payloads ────────────────────────────────────────────────────────

/** `pipeline-started` */
export interface PipelineStartedEvent {
  date: string;
  host: string;
  trigger: TriggerSource;
}

/** `pipeline-progress` */
export interface PipelineProgressEvent {
  date: string;
  host: string;
  step: string;
  /** 0..=100. */
  percent: number;
}

/** `pipeline-done` — the completed run's result. */
export type PipelineDoneEvent = CompileResult;

/** `pipeline-error` */
export interface PipelineErrorEvent {
  /** Matches the `AppError` variant tag. */
  code: string;
  message: string;
  /** Step that failed, e.g. `render_llm`. */
  step: string;
  date: string;
}

/** `scheduler-tick` */
export interface SchedulerTickEvent {
  /** ISO-8601. */
  next_run_at: string;
  source: SchedulerSource;
}
