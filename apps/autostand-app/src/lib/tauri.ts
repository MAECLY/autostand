/**
 * Typed wrappers over the 27 Tauri IPC commands and the 6 backend events.
 *
 * This is the only module in the app allowed to import from
 * `@tauri-apps/api` — everything else goes through `tauriApi` and the
 * `on*` helpers so argument names and payload shapes stay in one place.
 * Contract: `docs/tauri/02-ipc-contracts.md`.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  ApiKeyStatus,
  AppConfig,
  AuditData,
  AuditSidecar,
  CliDetection,
  CloudFolder,
  CompileResult,
  DataSourceConfig,
  GatherPreview,
  LlmProviderConfig,
  PathValidation,
  PipelineDoneEvent,
  PipelineErrorEvent,
  PipelineLogEvent,
  PipelineProgressEvent,
  PipelineStartedEvent,
  PipelineStatus,
  ProviderTestMode,
  RepoInfo,
  SchedulerStatus,
  SchedulerTickEvent,
  SettingsPaths,
  StandupFileContent,
  TestProviderResult,
} from "@/lib/types";

export const tauriApi = {
  getConfig: () => invoke<AppConfig>("get_config"),
  setConfig: (config: AppConfig) => invoke<void>("set_config", { config }),

  getHostSlug: () => invoke<string>("get_host_slug"),
  setHostSlug: (slug: string) => invoke<void>("set_host_slug", { slug }),

  listDataSources: () => invoke<DataSourceConfig[]>("list_data_sources"),
  toggleDataSource: (id: string, enabled: boolean) =>
    invoke<void>("toggle_data_source", { id, enabled }),

  listLlmProviders: () => invoke<LlmProviderConfig[]>("list_llm_providers"),
  testLlmProvider: (provider: string, mode: ProviderTestMode) =>
    invoke<TestProviderResult>("test_llm_provider", { provider, mode }),
  listProviderModels: (provider: string) =>
    invoke<string[]>("list_provider_models", { provider }),

  // `date` is `Option<String>` on the Rust side; null selects "today".
  compileStandup: (date?: string) =>
    invoke<CompileResult>("compile_standup", { date: date ?? null }),
  compileAll: () => invoke<CompileResult[]>("compile_all"),
  triggerRunNow: () => invoke<CompileResult>("trigger_run_now"),

  readStandupFile: (date: string) =>
    invoke<StandupFileContent>("read_standup_file", { date }),
  addManualItem: (date: string, item: string) =>
    invoke<void>("add_manual_item", { date, item }),

  listAuditSidecars: (date: string) =>
    invoke<AuditSidecar[]>("list_audit_sidecars", { date }),
  readAuditSidecar: (path: string) =>
    invoke<AuditData>("read_audit_sidecar", { path }),

  getPipelineStatus: () => invoke<PipelineStatus>("get_pipeline_status"),
  previewGather: (date: string) =>
    invoke<GatherPreview>("preview_gather", { date }),

  getSchedulerStatus: () => invoke<SchedulerStatus>("get_scheduler_status"),
  setSchedulerSchedule: (cron: string) =>
    invoke<void>("set_scheduler_schedule", { cron }),

  discoverRepos: () => invoke<RepoInfo[]>("discover_repos"),

  getSettingsPaths: () => invoke<SettingsPaths>("get_settings_paths"),
  validatePaths: () => invoke<PathValidation[]>("validate_paths"),

  detectCloudFolders: () => invoke<CloudFolder[]>("detect_cloud_folders"),

  storeApiKey: (provider: string, key: string) =>
    invoke<void>("store_api_key", { provider, key }),
  getApiKeyStatus: (provider: string) =>
    invoke<ApiKeyStatus>("get_api_key_status", { provider }),
  detectCli: (provider: string) =>
    invoke<CliDetection>("detect_cli", { provider }),
} as const;

// ── Events ────────────────────────────────────────────────────────────────

/** Build a listener that hands the handler the payload instead of the envelope. */
function eventHelper<T>(name: string) {
  return (handler: (payload: T) => void): Promise<UnlistenFn> =>
    listen<T>(name, (event) => handler(event.payload));
}

export const onPipelineStarted =
  eventHelper<PipelineStartedEvent>("pipeline-started");

export const onPipelineProgress =
  eventHelper<PipelineProgressEvent>("pipeline-progress");

export const onPipelineLog =
  eventHelper<PipelineLogEvent>("pipeline-log");

export const onPipelineDone = eventHelper<PipelineDoneEvent>("pipeline-done");

export const onPipelineError =
  eventHelper<PipelineErrorEvent>("pipeline-error");

export const onSchedulerTick =
  eventHelper<SchedulerTickEvent>("scheduler-tick");
