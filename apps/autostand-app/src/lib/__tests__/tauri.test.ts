/**
 * IPC contract regression test.
 *
 * Renaming a Rust command or one of its argument keys breaks the app silently
 * at runtime — TypeScript cannot see across the boundary. This table pins every
 * command name and argument object exactly as `docs/tauri/02-ipc-contracts.md`
 * declares them, so a drift shows up here first.
 */

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import {
  emitTauriEvent,
  makeAppConfig,
  mockInvoke,
  mockListen,
  resetTauriMocks,
  tauriListenerCount,
} from "@/test/mocks";
import {
  onPipelineDone,
  onPipelineError,
  onPipelineLog,
  onPipelineProgress,
  onPipelineStarted,
  onSchedulerTick,
  tauriApi,
} from "@/lib/tauri";

const CONFIG = makeAppConfig();

interface CommandCase {
  /** Rust command name. */
  command: string;
  call: () => Promise<unknown>;
  /** Argument object, or `undefined` when the wrapper passes none. */
  args?: Record<string, unknown>;
}

const COMMANDS: CommandCase[] = [
  { command: "get_config", call: () => tauriApi.getConfig() },
  {
    command: "set_config",
    call: () => tauriApi.setConfig(CONFIG),
    args: { config: CONFIG },
  },
  { command: "get_host_slug", call: () => tauriApi.getHostSlug() },
  {
    command: "set_host_slug",
    call: () => tauriApi.setHostSlug("mbp-miguel"),
    args: { slug: "mbp-miguel" },
  },
  { command: "list_data_sources", call: () => tauriApi.listDataSources() },
  {
    command: "toggle_data_source",
    call: () => tauriApi.toggleDataSource("github", true),
    args: { id: "github", enabled: true },
  },
  { command: "list_llm_providers", call: () => tauriApi.listLlmProviders() },
  {
    command: "test_llm_provider",
    call: () => tauriApi.testLlmProvider("claude", "cli"),
    args: { provider: "claude", mode: "cli" },
  },
  {
    command: "list_provider_models",
    call: () => tauriApi.listProviderModels("claude"),
    args: { provider: "claude" },
  },
  { command: "get_provider_health", call: () => tauriApi.getProviderHealth() },
  {
    command: "refresh_provider_health",
    call: () => tauriApi.refreshProviderHealth("grok"),
    args: { provider: "grok" },
  },
  {
    command: "compile_standup",
    call: () => tauriApi.compileStandup("2026-08-03"),
    args: { date: "2026-08-03" },
  },
  { command: "compile_all", call: () => tauriApi.compileAll() },
  { command: "trigger_run_now", call: () => tauriApi.triggerRunNow() },
  {
    command: "preview_regeneration",
    call: () => tauriApi.previewRegeneration("2026-08-03"),
    args: { date: "2026-08-03" },
  },
  {
    command: "apply_regeneration",
    call: () => tauriApi.applyRegeneration("token", "merge", "- merged"),
    args: { token: "token", resolution: "merge", mergedAuto: "- merged" },
  },
  {
    command: "read_standup_file",
    call: () => tauriApi.readStandupFile("2026-08-03"),
    args: { date: "2026-08-03" },
  },
  {
    command: "add_manual_item",
    call: () => tauriApi.addManualItem("2026-08-03", "- paired on FIF-136"),
    args: { date: "2026-08-03", item: "- paired on FIF-136" },
  },
  {
    command: "list_standup_dates",
    call: () => tauriApi.listStandupDates("2026-08-01", "2026-08-31"),
    args: { since: "2026-08-01", until: "2026-08-31" },
  },
  {
    command: "list_audit_sidecars",
    call: () => tauriApi.listAuditSidecars("2026-08-03"),
    args: { date: "2026-08-03" },
  },
  {
    command: "read_audit_sidecar",
    call: () => tauriApi.readAuditSidecar("/audit/2026-08-03.mbp.json"),
    args: { path: "/audit/2026-08-03.mbp.json" },
  },
  { command: "get_pipeline_status", call: () => tauriApi.getPipelineStatus() },
  {
    command: "preview_gather",
    call: () => tauriApi.previewGather("2026-08-03"),
    args: { date: "2026-08-03" },
  },
  { command: "get_scheduler_status", call: () => tauriApi.getSchedulerStatus() },
  {
    command: "set_scheduler_schedule",
    call: () => tauriApi.setSchedulerSchedule("0 9 * * 1-5"),
    args: { cron: "0 9 * * 1-5" },
  },
  {
    command: "set_scheduler_enabled",
    call: () => tauriApi.setSchedulerEnabled(true),
    args: { enabled: true },
  },
  { command: "discover_repos", call: () => tauriApi.discoverRepos() },
  { command: "get_settings_paths", call: () => tauriApi.getSettingsPaths() },
  { command: "validate_paths", call: () => tauriApi.validatePaths() },
  {
    command: "open_in_file_manager",
    call: () => tauriApi.openInFileManager("/home/tester/Documents/Github"),
    args: { path: "/home/tester/Documents/Github" },
  },
  { command: "detect_cloud_folders", call: () => tauriApi.detectCloudFolders() },
  {
    command: "configure_cloud_sync",
    call: () => tauriApi.configureCloudSync("/cloud"),
    args: { rootPath: "/cloud" },
  },
  { command: "get_repo_sync_status", call: () => tauriApi.getRepoSyncStatus() },
  {
    command: "setup_repo_sync",
    call: () => tauriApi.setupRepoSync("standups"),
    args: { repoName: "standups" },
  },
  {
    command: "store_api_key",
    call: () => tauriApi.storeApiKey("claude", "sk-test-value"),
    args: { provider: "claude", key: "sk-test-value" },
  },
  {
    command: "get_api_key_status",
    call: () => tauriApi.getApiKeyStatus("claude"),
    args: { provider: "claude" },
  },
  {
    command: "detect_cli",
    call: () => tauriApi.detectCli("claude"),
    args: { provider: "claude" },
  },
  {
    command: "get_notification_status",
    call: () => tauriApi.getNotificationStatus(),
  },
  {
    command: "request_notification_permission",
    call: () => tauriApi.requestNotificationPermission(),
  },
  {
    command: "send_test_notification",
    call: () => tauriApi.sendTestNotification(),
  },
  { command: "list_local_models", call: () => tauriApi.listLocalModels() },
  {
    command: "download_local_model",
    call: () => tauriApi.downloadLocalModel("gemma-3-1b"),
    args: { modelId: "gemma-3-1b" },
  },
  {
    command: "cancel_local_model_download",
    call: () => tauriApi.cancelLocalModelDownload("gemma-3-1b"),
    args: { modelId: "gemma-3-1b" },
  },
  {
    command: "delete_local_model",
    call: () => tauriApi.deleteLocalModel("gemma-3-1b"),
    args: { modelId: "gemma-3-1b" },
  },
  {
    command: "select_local_model",
    call: () => tauriApi.selectLocalModel("gemma-3-1b"),
    args: { modelId: "gemma-3-1b" },
  },
  {
    command: "accept_local_model_terms",
    call: () => tauriApi.acceptLocalModelTerms("gemma-3-1b"),
    args: { modelId: "gemma-3-1b" },
  },
  { command: "unload_local_models", call: () => tauriApi.unloadLocalModels() },
];

beforeEach(() => {
  resetTauriMocks();
});

describe("tauriApi", () => {
  it("exposes exactly the 47 documented commands", () => {
    expect(COMMANDS).toHaveLength(47);
    expect(Object.keys(tauriApi)).toHaveLength(47);
    expect(new Set(COMMANDS.map((entry) => entry.command)).size).toBe(47);
  });

  it.each(COMMANDS)(
    "$command is invoked with the documented arguments",
    async ({ command, call, args }) => {
      await call();

      expect(mockInvoke.mock.calls).toEqual([
        args === undefined ? [command] : [command, args],
      ]);
    },
  );

  it("passes a null date so the backend picks today", async () => {
    await tauriApi.compileStandup();

    expect(mockInvoke).toHaveBeenCalledWith("compile_standup", { date: null });
  });

  it("passes null optional regeneration fields", async () => {
    await tauriApi.previewRegeneration();
    expect(mockInvoke).toHaveBeenLastCalledWith("preview_regeneration", {
      date: null,
    });

    await tauriApi.applyRegeneration("token", "keep_current");
    expect(mockInvoke).toHaveBeenLastCalledWith("apply_regeneration", {
      token: "token",
      resolution: "keep_current",
      mergedAuto: null,
    });
  });

  it("resolves with whatever the backend returned", async () => {
    mockInvoke.mockResolvedValue(CONFIG);

    await expect(tauriApi.getConfig()).resolves.toBe(CONFIG);
  });

  it("stays in lockstep with the e2e mock-backend dispatcher", () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const mockPath = resolve(
      here,
      "../../../../../tests/e2e/support/mock-backend.ts",
    );
    const source = readFileSync(mockPath, "utf8");
    const handled = new Set(
      [...source.matchAll(/case "([a-z0-9_]+)"/g)].map((match) => match[1]),
    );
    const documented = COMMANDS.map((entry) => entry.command);
    expect([...handled].sort()).toEqual([...documented].sort());
  });

  it("rejects with the serialized AppError untouched", async () => {
    const appError = { code: "config", message: "missing dailies_dir" };
    mockInvoke.mockRejectedValue(appError);

    await expect(tauriApi.getConfig()).rejects.toEqual(appError);
  });
});

describe("event helpers", () => {
  /** The helpers differ only in payload type, which is contravariant here. */
  type Subscribe = (handler: (payload: unknown) => void) => Promise<() => void>;

  const EVENTS: { name: string; subscribe: Subscribe }[] = [
    { name: "pipeline-started", subscribe: onPipelineStarted },
    { name: "pipeline-progress", subscribe: onPipelineProgress },
    { name: "pipeline-log", subscribe: onPipelineLog },
    { name: "pipeline-done", subscribe: onPipelineDone },
    { name: "pipeline-error", subscribe: onPipelineError },
    { name: "scheduler-tick", subscribe: onSchedulerTick },
  ];

  it("covers all 6 backend events", () => {
    expect(EVENTS).toHaveLength(6);
  });

  it.each(EVENTS)(
    "$name unwraps the envelope and unsubscribes on demand",
    async ({ name, subscribe }) => {
      const handler = vi.fn();
      const unlisten = await subscribe(handler);

      expect(mockListen).toHaveBeenCalledWith(name, expect.any(Function));
      expect(tauriListenerCount(name)).toBe(1);

      const payload = { marker: name };
      emitTauriEvent(name, payload);
      expect(handler).toHaveBeenCalledTimes(1);
      expect(handler).toHaveBeenCalledWith(payload);

      unlisten();
      expect(tauriListenerCount(name)).toBe(0);

      emitTauriEvent(name, payload);
      expect(handler).toHaveBeenCalledTimes(1);
    },
  );
});
