# Tauri IPC Command Contracts

This document specifies the contract between the React frontend and the Rust backend of the `autostand` Tauri v2 app. Every backend callable is a `#[tauri::command]` function; every frontend call goes through a typed wrapper in `apps/autostand-app/src/lib/tauri.ts`.

---

## Architecture

```
┌────────────────────────────┐    invoke(cmd, args)    ┌─────────────────────────────┐
│  React (TypeScript)        │ ───────────────────────▶│  Rust (autostand-app)        │
│  @tauri-apps/api/core      │ ◀───────────────────────│  #[tauri::command]           │
│  invoke / listen           │   Result<T, AppError>    │  delegates to autostand-*     │
└────────────────────────────┘                          └─────────────────────────────┘
         │ listen("pipeline-progress", …)                          │ app_handle.emit(…)
         └──────────────────── events ─────────────────────────────┘
```

- **Frontend** imports `invoke` from `@tauri-apps/api/core`. All calls are routed through `src/lib/tauri.ts` wrappers that apply the TypeScript types in `src/lib/types.ts` (mirrors of Rust serde types).
- **Backend** exposes `#[tauri::command] async fn ...` functions grouped by domain under `src-tauri/src/commands/`. Async commands return `Result<T, AppError>` where `AppError` serializes to `{ code, message }` (see [Error handling](#error-handting)).
- **Events** are emitted by the backend via `app_handle.emit("pipeline-progress", payload)` and consumed on the frontend with `listen("pipeline-progress", handler)` from `@tauri-apps/api/event`.

### Type round-trip

Rust structs derive `serde::Serialize` + `serde::Deserialize`. The frontend mirrors each struct as a `type`/`interface` in `src/lib/types.ts`. A `specta`/`ts-rs` build step may be added later to auto-generate the TS side; until then both sides are kept in lockstep by the contract table below.

---

## Command inventory

| Command | Args (TS) | Return | Description | Delegates to |
| --- | --- | --- | --- | --- |
| `get_config` | — | `AppConfig` | Load app config from Tauri Store | `autostand-core::config::load` |
| `set_config` | `{ config: AppConfig }` | `void` | Persist config to store; re-validates paths | `autostand-core::config::save` |
| `get_host_slug` | — | `string` | Return persisted host slug; if absent, detect + persist | `autostand-core::host::detect_or_load` |
| `set_host_slug` | `{ slug: string }` | `void` | Manual override; rejects numeric/IP-like | `autostand-core::host::persist` |
| `list_data_sources` | — | `DataSourceConfig[]` | List configured data sources + enabled state | `autostand-core::config::data_sources` |
| `toggle_data_source` | `{ id: string, enabled: boolean }` | `void` | Flip a source flag, persist config | `autostand-core::config::set_source` |
| `list_llm_providers` | — | `LlmProviderConfig[]` | List 6 providers with status: CLI detected? API key set? | `autostand-adapters::llm::registry` |
| `test_llm_provider` | `{ provider: string, mode: "cli" \| "api" }` | `{ ok: boolean, message: string, latency_ms: number }` | Ping provider (echo prompt); never throws | `autostand-adapters::llm::test` |
| `list_provider_models` | `{ provider: string }` | `string[]` | Probe the provider API for model ids. Empty on missing key / unreachable host; `invalid` only for an unknown provider | `autostand-adapters::llm::helpers` |
| `get_provider_health` | — | `ProviderHealth[]` | Probe supported usage sources for all providers and merge safe inferred failures | `commands::llm` |
| `refresh_provider_health` | `{ provider: string \| null }` | `ProviderHealth[]` | Refresh one provider or all; emits `provider-health-updated` | `commands::llm` |
| `list_local_models` | — | `LocalModelInfo[]` | List the pinned built-in catalog and derived on-disk status | `commands::local_models` |
| `download_local_model` | `{ modelId: string }` | `void` | Start/resume a user-initiated, hash-verified model download | `commands::local_models` |
| `cancel_local_model_download` | `{ modelId: string }` | `boolean` | Signal cancellation and retain the partial file for resume | `commands::local_models` |
| `delete_local_model` | `{ modelId: string }` | `void` | Delete final and partial model files and clear selection when applicable | `commands::local_models` |
| `select_local_model` | `{ modelId: string }` | `void` | Select an installed, size-valid catalog model | `commands::local_models` |
| `accept_local_model_terms` | `{ modelId: string }` | `void` | Persist acceptance of the catalog's exact terms version | `commands::local_models` |
| `unload_local_models` | — | `LocalRuntimeUnload` | Terminate any process still holding a managed GGUF and delete the reusable prompt caches; model files are kept | `commands::local_models` |
| `get_notification_status` | — | `NotificationStatus` | Read OS permission and saved notification preferences without prompting | `notifications` |
| `request_notification_permission` | — | `string` | Ask for OS permission after an explicit Settings action | `notifications` |
| `send_test_notification` | — | `boolean` | Send a content-free test alert; respects master opt-in and dedup policy | `notifications` |
| `compile_standup` | `{ date?: string }` | `CompileResult` | Run full pipeline for one date (default: today) | `autostand-core::pipeline::trigger` |
| `compile_all` | — | `CompileResult[]` | Recompile F_TODAY + F_PREV (business-day aware) | `autostand-core::pipeline::trigger_all` |
| `preview_regeneration` | `{ date?: string }` | `RegenerationPreview` | Generate a fresh, isolated F_TODAY candidate and return current/candidate AUTO bodies without writing the live standup | `autostand-app::pipeline_runner::compile_one(Preview)` |
| `apply_regeneration` | `{ token: string, resolution: "keep_current" \| "use_candidate" \| "merge", mergedAuto?: string }` | `RegenerationApplied` | Apply an explicit resolution after expiry/base-hash checks; replaces only this host's AUTO block | `autostand-core::fileops::set_auto` |
| `read_standup_file` | `{ date: string }` | `StandupFileContent` | Parse `dailies/<date>.md` → AUTO blocks per host, MANUAL region, title, subtitle | `autostand-core::format::parse_file` |
| `add_manual_item` | `{ date: string, item: string }` | `void` | Append line to MANUAL region of `<date>.md` (atomic) | `autostand-core::format::append_manual` |
| `list_standup_dates` | `{ since: string, until: string }` | `string[]` | One `read_dir` of `dailies_dir`; `YYYY-MM-DD.md` stems in the inclusive range | `std::fs::read_dir` |
| `list_audit_sidecars` | `{ date: string }` | `AuditSidecar[]` | List `state/audit/<date>-*.json` files | `autostand-core::audit::list_for_date` |
| `read_audit_sidecar` | `{ path: string }` | `AuditData` | Parse one sidecar JSON | `autostand-core::audit::read` |
| `get_pipeline_status` | — | `PipelineStatus` | Current run state (idle/gathering/rendering/done/error) + last run info | `autostand-app::state::status` |
| `preview_gather` | `{ date: string }` | `GatherPreview` | Show raw gathered FACTS/NOTES/ENRICHMENT without rendering (debug UI) | `autostand-core::pipeline::gather_only` |
| `get_scheduler_status` | — | `SchedulerStatus` | Next run time, last run time, trigger source, schedule source (system/in-process) | `autostand-scheduler::status` |
| `set_scheduler_schedule` | `{ cron: string }` | `void` | Persist cron + reinstall system unit | `autostand-scheduler::set_schedule` |
| `trigger_run_now` | — | `CompileResult` | Manually trigger a compile outside the cron schedule | `autostand-core::pipeline::trigger(Manual)` |
| `discover_repos` | — | `RepoInfo[]` | Scan `GITHUB_DIR` for git repos (depth-1) | `autostand-adapters::git::discover` |
| `get_settings_paths` | — | `SettingsPaths` | Return all configured paths (GITHUB_DIR, dailies dir, claude dir, etc.) | `autostand-core::config::paths` |
| `validate_paths` | — | `PathValidation[]` | Check each path exists + readable; returns per-path ok/missing | `autostand-core::config::validate` |
| `open_in_file_manager` | `{ path: string }` | `void` | Open a directory in the OS file manager (Finder / Explorer / `xdg-open`). Rejects blank, relative, and non-directory paths before the shell handoff: `invalid`, or `not_found` when the directory is gone | `tauri_plugin_opener::open_path` |
| `store_api_key` | `{ provider: string, key: string }` | `void` | Store key in OS keychain (`keyring` crate) under `autostand.<provider>` | `autostand-app::secrets::store` |
| `get_api_key_status` | `{ provider: string }` | `{ set: boolean, mode: "keychain" \| "env" \| "none" }` | Whether a key is set and where it came from | `autostand-app::secrets::status` |
| `detect_cli` | `{ provider: string }` | `{ found: boolean, path: string, version: string }` | Locate CLI binary on PATH + `--version` probe | `autostand-adapters::cli::detect` |

---

## TypeScript type definitions

These interfaces live in `apps/autostand-app/src/lib/types.ts` and are the canonical contract the UI imports.

```ts
// src/lib/types.ts

export interface AppConfig {
  github_dir: string;
  dailies_dir: string;
  standup_authors: string[];
  git_refs: string;
  jira_base: string;
  host_slug_override: string | null;
  render_mode: "Auto" | "Llm" | "Det";
  llm: LlmConfig;
  data_sources: DataSourceConfigs;
  scheduler: SchedulerConfig;
  review: ReviewConfig;
  scrub: ScrubConfig;
  format: StandupFormatConfig;
  notifications: NotificationConfig;
  sync: { cloud_root: string | null; repo_enabled: boolean };
  regeneration: { replace_immediately: boolean };
}

export interface LlmConfig {
  preferred_provider: string;
  providers: ProviderConfig[];
  fallback_enabled: boolean;
  provider_order: string[];
  fallback_policy: ProviderFallbackPolicy;
  local_runtime_policy: "on_demand" | "keep_ready";
}

export interface ProviderFallbackPolicy {
  retry_rate_limits: boolean;
  max_retry_after_secs: number;
}

export interface NotificationConfig {
  enabled: boolean;
  low_usage: boolean;
  low_usage_threshold_percent: number;
  provider_exhausted: boolean;
  provider_fallback: boolean;
  local_model_downloads: boolean;
  standup_complete: boolean;
  standup_failed: boolean;
}

export interface NotificationStatus {
  supported: boolean;
  permission: string;
  config: NotificationConfig;
}

export interface ProviderConfig {
  id: string;          // cloud/CLI provider id or "builtin-local"
  enabled: boolean;
  mode: "CliFirst" | "ApiFallback" | "CliOnly" | "ApiOnly";
  model: string;
  cli_path: string | null;
  api_key_ref: string | null;   // keychain reference; never the key itself
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

export interface DataSourceConfig {
  id: string;
  label: string;
  enabled: boolean;
  description: string;
}

export interface LlmProviderConfig {
  id: string;
  label: string;
  enabled: boolean;
  mode: "CliFirst" | "ApiFallback" | "CliOnly" | "ApiOnly";
  model: string;
  cli: { found: boolean; path: string; version: string };
  api_key: { set: boolean; mode: "keychain" | "env" | "none" };
}

export type UsageSource =
  | "provider_reported"
  | "response_headers"
  | "management_api"
  | "failure_inferred"
  | "unknown";

export type ProviderAvailability =
  | "available"
  | "low"
  | "exhausted"
  | "rate_limited"
  | "auth_required"
  | "model_unavailable"
  | "unavailable"
  | "unknown";

export interface UsageWindow {
  id: string;
  used_percent: number | null;
  remaining_percent: number | null;
  resets_at: string | null;
}

export interface ProviderHealth {
  provider: string;
  availability: ProviderAvailability;
  source: UsageSource;
  windows: UsageWindow[];
  reason: string | null;
  checked_at: string;
}

export type LocalModelStatus =
  | "not_downloaded"
  | "downloading"
  | "available"
  | "corrupted"
  | "error";

export interface LocalModelInfo {
  id: string;
  display_name: string;
  tier: string;
  quality: string;
  format: "GGUF";
  size_bytes: number;
  context_length: number;
  status: LocalModelStatus;
  selected: boolean;
  license: string;
  license_url: string;
  terms_required: boolean;
  downloaded_bytes: number;
  runtime_cache_bytes: number;   // reusable llama.cpp prompt/KV cache, 0 when cold
  error: string | null;
}

export interface LocalRuntimeUnload {
  processes_terminated: number;
  caches_removed: number;
  bytes_freed: number;
}

export interface LocalModelProgressEvent {
  model_id: string;
  status: LocalModelStatus;
  downloaded_bytes: number;
  total_bytes: number;
  bytes_per_second: number;
  error: string | null;
}

export interface CompileResult {
  date: string;                 // "2026-08-03"
  host: string;
  status: "ok" | "skip" | "error";
  render_used: "llm" | "det" | "llm_fallback";
  fellback: boolean;
  audit_path: string | null;
  file_path: string;
  accumulated_count: number;    // bullets re-injected from PREV
  message: string;
}

export interface RegenerationPreview {
  token: string;                // opaque; expires after 30 minutes
  date: string;
  host: string;
  current_auto: string;
  candidate_auto: string;
  base_hash: string;            // exact live-file SHA-256 used for TOCTOU protection
  expires_at: string;
  render_used: "llm" | "det" | "llm_fallback";
  fellback: boolean;
  message: string;
}

export type RegenerationResolution = "keep_current" | "use_candidate" | "merge";

export interface RegenerationApplied {
  date: string;
  host: string;
  file_path: string;
  resolution: RegenerationResolution;
  auto_body: string;
  committed: boolean;
  pushed: boolean;
  message: string;
}

export interface StandupFileContent {
  date: string;
  title: string;                // "Daily Standup — August 03, 2026"
  subtitle: string;             // "_Work completed August 01–02, 2026._"
  auto_blocks: AutoBlock[];     // one per host slug
  manual_region: string;        // verbatim inner content of MANUAL block
}

export interface AutoBlock {
  host: string;
  body: string;                 // verbatim inner content of AUTO block
}

export interface AuditSidecar {
  path: string;
  date: string;
  host: string;
  rendered_at: string;          // ISO-8601 UTC
  render_used: "llm" | "det" | "llm_fallback";
}

export interface AuditData {
  file: string;
  host: string;
  rendered_at: string;           // ISO-8601 UTC
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
  provider_attempts: ProviderAttempt[];
  fellback: boolean;
  hash: string;
  accumulated_count: number;
}

export interface ProviderAttempt {
  provider: string;
  channel: "cli" | "api" | null;
  model: string;
  status: "succeeded" | "failed" | "empty" | "skipped";
  reason: string | null;
  latency_ms: number | null;
}

export interface RepoFacts {
  repo: string;
  ticket: string | null;
  title: string;
  commits: { sha: string; subject: string; date: string; files: string[] }[];
}

export interface NoteRef {
  source: string;               // path to note file
  date: string;
  clauses: string[];
}

export interface SkewRecord {
  ticket: string;
  note_date: string;
  commit_days: string[];
}

export interface PipelineStatus {
  state: "idle" | "gathering" | "rendering" | "done" | "error";
  current_date: string | null;
  current_host: string | null;
  step: string | null;          // human-readable step name
  percent: number;               // 0..100
  last_run_at: string | null;    // ISO-8601
  last_result: CompileResult | null;
  error: string | null;
}

export interface GatherPreview {
  date: string;
  host: string;
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
}

export interface SchedulerStatus {
  enabled: boolean;
  source: "launchd" | "systemd" | "task-scheduler" | "in-process" | "none";
  cron: string;
  next_run_at: string | null;    // ISO-8601
  last_run_at: string | null;    // ISO-8601
  last_trigger: "scheduled" | "manual" | "self-heal" | null;
}

export interface RepoInfo {
  path: string;
  name: string;
  remote: string | null;
  last_commit_at: string | null; // ISO-8601
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
  label: string;
  exists: boolean;
  readable: boolean;
  message: string | null;
}
```

---

## Frontend wrappers (`lib/tauri.ts`)

Each command is wrapped so the UI never calls `invoke` directly — this is where retries, error mapping, and event bridging live.

```ts
// src/lib/tauri.ts
import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig, CompileResult, LlmProviderConfig, DataSourceConfig,
  StandupFileContent, AuditSidecar, AuditData, PipelineStatus,
  GatherPreview, SchedulerStatus, RepoInfo, SettingsPaths, PathValidation,
} from "@/lib/types";

export const tauriApi = {
  getConfig:            ()                          => invoke<AppConfig>("get_config"),
  setConfig:            (config: AppConfig)         => invoke<void>("set_config", { config }),
  getHostSlug:         ()                          => invoke<string>("get_host_slug"),
  setHostSlug:         (slug: string)              => invoke<void>("set_host_slug", { slug }),
  listDataSources:     ()                          => invoke<DataSourceConfig[]>("list_data_sources"),
  toggleDataSource:    (id: string, enabled: boolean) =>
                          invoke<void>("toggle_data_source", { id, enabled }),
  listLlmProviders:    ()                          => invoke<LlmProviderConfig[]>("list_llm_providers"),
  testLlmProvider:     (provider: string, mode: "cli" | "api") =>
                          invoke<{ ok: boolean; message: string; latency_ms: number }>(
                            "test_llm_provider", { provider, mode }),
  listProviderModels:  (provider: string)          =>
                          invoke<string[]>("list_provider_models", { provider }),
  compileStandup:       (date?: string)             => invoke<CompileResult>("compile_standup", { date }),
  compileAll:          ()                          => invoke<CompileResult[]>("compile_all"),
  readStandupFile:      (date: string)              => invoke<StandupFileContent>("read_standup_file", { date }),
  addManualItem:       (date: string, item: string) => invoke<void>("add_manual_item", { date, item }),
  listStandupDates:    (since: string, until: string) =>
                          invoke<string[]>("list_standup_dates", { since, until }),
  listAuditSidecars:   (date: string)               => invoke<AuditSidecar[]>("list_audit_sidecars", { date }),
  readAuditSidecar:    (path: string)               => invoke<AuditData>("read_audit_sidecar", { path }),
  getPipelineStatus:   ()                          => invoke<PipelineStatus>("get_pipeline_status"),
  previewGather:       (date: string)               => invoke<GatherPreview>("preview_gather", { date }),
  getSchedulerStatus:  ()                          => invoke<SchedulerStatus>("get_scheduler_status"),
  setSchedulerSchedule:(cron: string)              => invoke<void>("set_scheduler_schedule", { cron }),
  setSchedulerEnabled:(enabled: boolean)            => invoke<void>("set_scheduler_enabled", { enabled }),
  triggerRunNow:       ()                          => invoke<CompileResult>("trigger_run_now"),
  previewRegeneration:(date?: string)               => invoke<RegenerationPreview>("preview_regeneration", { date }),
  applyRegeneration:  (token, resolution, mergedAuto?) => invoke<RegenerationApplied>("apply_regeneration", { token, resolution, mergedAuto }),
  discoverRepos:       ()                          => invoke<RepoInfo[]>("discover_repos"),
  getSettingsPaths:    ()                          => invoke<SettingsPaths>("get_settings_paths"),
  validatePaths:       ()                          => invoke<PathValidation[]>("validate_paths"),
  openInFileManager:  (path: string)                => invoke<void>("open_in_file_manager", { path }),
  detectCloudFolders:  ()                          => invoke<CloudFolder[]>("detect_cloud_folders"),
  configureCloudSync: (rootPath: string)            => invoke<CloudSyncSelection>("configure_cloud_sync", { rootPath }),
  getRepoSyncStatus:  ()                           => invoke<RepoSyncStatus>("get_repo_sync_status"),
  setupRepoSync:      (repoName?: string)           => invoke<RepoSyncStatus>("setup_repo_sync", { repoName }),
  storeApiKey:         (provider: string, key: string) =>
                          invoke<void>("store_api_key", { provider, key }),
  getApiKeyStatus:     (provider: string) =>
                          invoke<{ set: boolean; mode: "keychain" | "env" | "none" }>(
                            "get_api_key_status", { provider }),
  detectCli:           (provider: string) =>
                          invoke<{ found: boolean; path: string; version: string }>(
                            "detect_cli", { provider }),
} as const;
```

---

## Event system

The backend emits events using the Tauri app handle. The frontend subscribes with `listen` from `@tauri-apps/api/event`.

### Events

| Event | Payload | Emitted by | When |
| --- | --- | --- | --- |
| `pipeline-started` | `{ date: string, host: string, trigger: "scheduled" \| "manual" \| "self-heal" }` | `pipeline::trigger` | After lock acquired, before compute_targets |
| `pipeline-progress` | `{ date: string, host: string, step: string, percent: number }` | each step | Before each pipeline step in `compile_file` |
| `pipeline-done` | `CompileResult` | `pipeline::trigger` | After commit_push (or skip) |
| `pipeline-error` | `{ code: string, message: string, step: string, date: string }` | `pipeline::trigger` catch | On any step failure that aborts the run |
| `scheduler-tick` | `{ next_run_at: string, source: string }` | `autostand-scheduler` | On each scheduler poll (every 60s in-process, or on unit activation) |
| `provider-health-updated` | `ProviderHealth[]` | `refresh_provider_health` | After an explicit/all-provider usage probe |
| `local-model-progress` | `LocalModelProgressEvent` | local model downloader | While downloading, after verification, or on corruption |

### Backend emit (Rust)

```rust
use tauri::{AppHandle, Emitter};

app_handle.emit("pipeline-progress", PipelineProgress {
    date: f.clone(),
    host: host.clone(),
    step: "render_llm".into(),
    percent: 72,
})?;
```

### Frontend listener

```ts
// src/hooks/use-pipeline-status.ts
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";

export function usePipelineEvents() {
  const qc = useQueryClient();
  useEffect(() => {
    const unsubs = [
      listen("pipeline-progress", (e) => {
        qc.setQueryData(["pipeline-status"], e.payload);
      }),
      listen("pipeline-done", (e) => {
        qc.invalidateQueries({ queryKey: ["standup", e.payload.date] });
        qc.invalidateQueries({ queryKey: ["audit", e.payload.date] });
      }),
      listen("pipeline-error", (e) => {
        toast.error(`${e.payload.step}: ${e.payload.message}`);
      }),
    ];
    return () => unsubs.forEach((u) => u.then((fn) => fn()));
  }, [qc]);
}
```

---

## Error handling

### Rust side

All commands return `Result<T, AppError>`. `AppError` is a single error enum that serializes to a stable JSON shape so the frontend can branch on `code`.

```rust
// crates/autostand-core/src/error.rs
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "code", content = "message")]
pub enum AppError {
    #[error("config: {0}")]
    Config(String),
    #[error("io: {0}")]
    Io(String),
    #[error("git: {0}")]
    Git(String),
    #[error("llm: {0}")]
    Llm(String),
    #[error("lock: {0}")]
    Lock(String),
    #[error("not_found: {0}")]
    NotFound(String),
    #[error("invalid: {0}")]
    Invalid(String),
}

// Tauri serializes the Err variant via serde → frontend receives { code, message }.
```

### Frontend side

`invoke` rejects with the serialized `AppError`. A single `lib/error.ts` maps it to a `Sonner` toast:

```ts
import { toast } from "sonner";

export function handleInvokeError(err: unknown, ctx = "") {
  const e = err as { code?: string; message?: string };
  const code = e?.code ?? "unknown";
  const msg  = e?.message ?? String(err);
  toast.error(`${ctx ? ctx + " — " : ""}${code}: ${msg}`);
}
```

### Step-granular errors

Inside `compile_file`, a step failure does **not** abort the whole pipeline:

- A failed LLM render → `render_llm` returns `None` → `validate_render` errors → fall back to `det_body`. A `pipeline-error` event is still emitted with `step: "render_llm"` and `code: "llm"`, but `CompileResult.status` stays `ok` with `render_used: "llm_fallback"`.
- A failed gather step (e.g. `gh` not on PATH) → enrichment for that source is skipped and recorded in the audit sidecar; the run continues with available sources.
- A `Lock` error aborts the run with `status: "error"` because another compile is already in progress.

This guarantees a standup is always produced if any facts/notes exist — see `docs/specs/pipeline.md` for the full step ordering.
