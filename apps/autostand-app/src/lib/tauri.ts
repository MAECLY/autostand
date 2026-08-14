/**
 * Typed wrappers over the 28 Tauri IPC commands and the 6 backend events.
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
  CloudSyncSelection,
  CompileResult,
  DataSourceConfig,
  Dependency,
  DependencyGroup,
  GatherPreview,
  LlmProviderConfig,
  LocalModelInfo,
  LocalModelProgressEvent,
  LocalRuntimeUnload,
  NotificationStatus,
  PathValidation,
  PipelineDoneEvent,
  PipelineErrorEvent,
  PipelineLogEvent,
  PipelineProgressEvent,
  PipelineStartedEvent,
  PipelineStatus,
  ProviderHealth,
  ProviderTestMode,
  RepoInfo,
  RegenerationApplied,
  RegenerationPreview,
  RegenerationResolution,
  RemediationOutcome,
  RepoSyncStatus,
  SchedulerStatus,
  SchedulerTickEvent,
  SettingsPaths,
  StandupFileContent,
  StandupReadiness,
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
  getProviderHealth: () => invoke<ProviderHealth[]>("get_provider_health"),
  refreshProviderHealth: (provider?: string) =>
    invoke<ProviderHealth[]>("refresh_provider_health", {
      provider: provider ?? null,
    }),

  // `date` is `Option<String>` on the Rust side; null selects "today".
  compileStandup: (date?: string) =>
    invoke<CompileResult>("compile_standup", { date: date ?? null }),
  compileAll: () => invoke<CompileResult[]>("compile_all"),
  triggerRunNow: () => invoke<CompileResult>("trigger_run_now"),
  previewRegeneration: (date?: string) =>
    invoke<RegenerationPreview>("preview_regeneration", {
      date: date ?? null,
    }),
  applyRegeneration: (
    token: string,
    resolution: RegenerationResolution,
    mergedAuto?: string,
  ) =>
    invoke<RegenerationApplied>("apply_regeneration", {
      token,
      resolution,
      mergedAuto: mergedAuto ?? null,
    }),

  readStandupFile: (date: string) =>
    invoke<StandupFileContent>("read_standup_file", { date }),
  addManualItem: (date: string, item: string) =>
    invoke<void>("add_manual_item", { date, item }),
  listStandupDates: (since: string, until: string) =>
    invoke<string[]>("list_standup_dates", { since, until }),

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
  setSchedulerEnabled: (enabled: boolean) =>
    invoke<void>("set_scheduler_enabled", { enabled }),

  discoverRepos: () => invoke<RepoInfo[]>("discover_repos"),
  getStandupReadiness: () =>
    invoke<StandupReadiness>("get_standup_readiness"),

  getSettingsPaths: () => invoke<SettingsPaths>("get_settings_paths"),
  validatePaths: () => invoke<PathValidation[]>("validate_paths"),
  openInFileManager: (path: string) =>
    invoke<void>("open_in_file_manager", { path }),

  detectCloudFolders: () => invoke<CloudFolder[]>("detect_cloud_folders"),
  configureCloudSync: (rootPath: string) =>
    invoke<CloudSyncSelection>("configure_cloud_sync", { rootPath }),
  getRepoSyncStatus: () =>
    invoke<RepoSyncStatus>("get_repo_sync_status"),
  setupRepoSync: (repoName?: string) =>
    invoke<RepoSyncStatus>("setup_repo_sync", {
      repoName: repoName?.trim() || null,
    }),

  // Each call spawns child processes on the Rust side; cache aggressively.
  getDependencyStatus: (group?: DependencyGroup) =>
    invoke<Dependency[]>("get_dependency_status", { group: group ?? null }),
  runDependencyRemediation: (dependencyId: string) =>
    invoke<RemediationOutcome>("run_dependency_remediation", { dependencyId }),

  storeApiKey: (provider: string, key: string) =>
    invoke<void>("store_api_key", { provider, key }),
  getApiKeyStatus: (provider: string) =>
    invoke<ApiKeyStatus>("get_api_key_status", { provider }),
  detectCli: (provider: string) =>
    invoke<CliDetection>("detect_cli", { provider }),
  getNotificationStatus: () =>
    invoke<NotificationStatus>("get_notification_status"),
  requestNotificationPermission: () =>
    invoke<string>("request_notification_permission"),
  sendTestNotification: () => invoke<boolean>("send_test_notification"),
  listLocalModels: () => invoke<LocalModelInfo[]>("list_local_models"),
  downloadLocalModel: (modelId: string) =>
    invoke<void>("download_local_model", { modelId }),
  cancelLocalModelDownload: (modelId: string) =>
    invoke<void>("cancel_local_model_download", { modelId }),
  deleteLocalModel: (modelId: string) =>
    invoke<void>("delete_local_model", { modelId }),
  selectLocalModel: (modelId: string) =>
    invoke<void>("select_local_model", { modelId }),
  acceptLocalModelTerms: (modelId: string) =>
    invoke<void>("accept_local_model_terms", { modelId }),
  unloadLocalModels: () => invoke<LocalRuntimeUnload>("unload_local_models"),
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

export const onProviderHealthUpdated =
  eventHelper<ProviderHealth[]>("provider-health-updated");

export const onLocalModelProgress =
  eventHelper<LocalModelProgressEvent>("local-model-progress");
