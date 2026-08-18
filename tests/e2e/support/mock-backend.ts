/**
 * The fake autostand backend, as it exists inside the browser page.
 *
 * `installMockBackend` is serialised by `page.addInitScript` and evaluated
 * before any application script runs, so it must be entirely self-contained:
 * every helper lives inside the function body and the only imports are
 * type-only (erased at transpile time).
 *
 * It routes `invoke` through the real `mockIPC` from `@tauri-apps/api/mocks`
 * — loaded by the preceding init script — and answers each documented IPC
 * commands from a mutable state object. Commands are dispatched by their exact
 * contract names; anything unlisted rejects loudly rather than resolving
 * `undefined`, so a renamed command fails a spec instead of quietly emptying
 * the UI.
 */

import type {
  AppConfig,
  AppError,
  PipelineDoneEvent,
  PipelineErrorEvent,
  PipelineProgressEvent,
  PipelineStartedEvent,
  RunFinishedEvent,
  RunStartedEvent,
} from "../../../apps/autostand-app/src/lib/types";
import type { BackendState, Scenario } from "./scenario";

/** One recorded `invoke`, in call order. */
export interface RecordedCall {
  command: string;
  args: Record<string, unknown>;
}

/** The control surface the Playwright fixture drives from Node. */
export interface E2EBridge {
  state: BackendState;
  calls: RecordedCall[];
  emit: (name: string, payload: unknown) => void;
  settle: (command: string, result: unknown) => boolean;
  fail: (command: string, error: AppError) => boolean;
  isPending: (command: string) => boolean;
  callsTo: (command: string) => Record<string, unknown>[];
  patchState: (patch: Partial<BackendState>) => void;
}

/**
 * Global the CJS shim parks `@tauri-apps/api/mocks` under. The function body
 * below spells it out literally — it is serialised, so it cannot close over
 * this constant — but the shim script in `fixtures.ts` reads it from here.
 */
export const MOCKS_GLOBAL = "__TAURI_MOCKS__";

export function installMockBackend(scenario: Scenario): void {
  type MockIPC = (
    handler: (command: string, args?: Record<string, unknown>) => unknown,
    options?: { shouldMockEvents?: boolean },
  ) => void;

  const host = globalThis as unknown as {
    __TAURI_MOCKS__?: { mockIPC?: MockIPC };
    __E2E__?: E2EBridge;
    __TAURI_INTERNALS__?: {
      invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
    };
    exports?: unknown;
  };

  const mockIPC = host.__TAURI_MOCKS__?.mockIPC;
  if (typeof mockIPC !== "function") {
    throw new Error(
      "@tauri-apps/api/mocks was not loaded before installMockBackend — check the init script order",
    );
  }
  // The `exports` shim only had to exist while the CommonJS mocks build
  // evaluated. Drop it before any page script runs: UMD bundles sniff for a
  // global `exports` and would register themselves there instead of on window.
  delete host.exports;

  const state = scenario.state;
  const calls: RecordedCall[] = [];
  const pending = new Map<
    string,
    { resolve: (value: unknown) => void; reject: (reason: unknown) => void }
  >();

  /** Reject the way a Rust `AppError` crosses the boundary: `{ code, message }`. */
  function reject(code: string, message: string): never {
    const error: AppError = { code, message };
    throw error;
  }

  function findProvider(id: string) {
    return state.providers.find((provider) => provider.id === id);
  }

  // ── filing dates ────────────────────────────────────────────────────────
  //
  // A working copy of `autostand_core::dates`, because `get_filing_target` is
  // the one command whose *answer* the dashboard renders as a fact rather than
  // passing through. A lookup table would let a spec assert a target the rule
  // never produces, so the mock derives it the same way the backend does.
  // Everything is UTC: the Playwright config pins the browser to it.

  const DAY_MS = 86_400_000;

  function parseIso(iso: string): Date {
    const [year, month, day] = iso.split("-").map(Number);
    return new Date(Date.UTC(year, month - 1, day));
  }

  function toIso(date: Date): string {
    return date.toISOString().slice(0, 10);
  }

  function addDays(date: Date, days: number): Date {
    return new Date(date.getTime() + days * DAY_MS);
  }

  function isWeekend(date: Date): boolean {
    const weekday = date.getUTCDay();
    return weekday === 0 || weekday === 6;
  }

  /** Fri +3, Sat +2, Sun +1, otherwise +1. */
  function nextBusinessDay(date: Date): Date {
    const weekday = date.getUTCDay();
    if (weekday === 5) return addDays(date, 3);
    if (weekday === 6) return addDays(date, 2);
    return addDays(date, 1);
  }

  /** The latest weekday strictly before `date`. */
  function prevBusinessDayBefore(date: Date): Date {
    let cursor = addDays(date, -1);
    while (isWeekend(cursor)) cursor = addDays(cursor, -1);
    return cursor;
  }

  function filingDateFor(workDay: Date, mode: AppConfig["dates"]["archive_mode"]): Date {
    if (mode === "next_business_day") return nextBusinessDay(workDay);
    return isWeekend(workDay) ? nextBusinessDay(workDay) : workDay;
  }

  function makeFilingTarget(workDayIso: string) {
    const mode = state.config.dates?.archive_mode ?? "next_business_day";
    const workDay = parseIso(workDayIso);
    const filing = filingDateFor(workDay, mode);
    const today = parseIso(toIso(new Date()));

    const start =
      mode === "next_business_day"
        ? prevBusinessDayBefore(filing)
        : addDays(prevBusinessDayBefore(filing), 1);
    const lastClaimable =
      mode === "next_business_day" ? addDays(filing, -1) : filing;
    const end = lastClaimable.getTime() < today.getTime() ? lastClaimable : today;

    return {
      work_day: toIso(workDay),
      filing_date: toIso(filing),
      archive_mode: mode,
      window: { range_start: toIso(start), range_end: toIso(end) },
      window_empty: end.getTime() < start.getTime(),
    };
  }

  function dispatch(command: string, args: Record<string, unknown>): unknown {
    switch (command) {
      case "get_config":
        return state.config;
      case "set_config":
        state.config = args.config as AppConfig;
        return null;

      case "get_host_slug":
        return state.hostSlug;
      case "set_host_slug":
        state.hostSlug = String(args.slug);
        return null;

      case "list_data_sources":
        return state.dataSources;
      case "toggle_data_source": {
        const id = String(args.id);
        const source = state.dataSources.find((entry) => entry.id === id);
        if (source === undefined) reject("not_found", `unknown data source: ${id}`);
        if (id === "local-git") {
          reject(
            "invalid_config",
            "local-git is authoritative and cannot be disabled",
          );
        }
        source.enabled = Boolean(args.enabled);
        return null;
      }

      case "list_llm_providers":
        return state.providers;
      case "list_provider_models":
        return state.providerModels[String(args.provider)] ?? [];
      case "test_llm_provider": {
        const id = String(args.provider);
        const result = state.providerTests[id];
        if (result === undefined) {
          reject("not_found", `no scripted test result for provider: ${id}`);
        }
        return result;
      }
      case "get_provider_health":
      case "refresh_provider_health":
        return state.providerHealth;

      case "compile_standup":
      case "trigger_run_now":
        return state.compileResult;
      case "compile_all":
        return [state.compileResult];
      case "preview_regeneration": {
        const date = args.date === null ? state.compileResult.date : String(args.date);
        const file = state.standups[date];
        const current =
          file?.auto_blocks.find((block) => block.host === state.hostSlug)?.body ?? "";
        return {
          token: "a".repeat(64),
          date,
          host: state.hostSlug,
          current_auto: current,
          candidate_auto: "**Candidate**\n- Regenerated work",
          base_hash: "b".repeat(64),
          expires_at: "2026-08-03T12:30:00Z",
          render_used: "llm",
          fellback: false,
          message: "regeneration preview",
        };
      }
      case "apply_regeneration":
        return {
          date: state.compileResult.date,
          host: state.hostSlug,
          file_path: state.compileResult.file_path,
          resolution: args.resolution,
          auto_body: args.mergedAuto ?? "**Candidate**\n- Regenerated work",
          committed: false,
          pushed: false,
          message: "regeneration applied",
        };

      case "read_standup_file": {
        const date = String(args.date);
        const file = state.standups[date];
        // The dashboard's empty state keys off exactly this code.
        if (file === undefined) reject("not_found", `no standup file for ${date}`);
        return file;
      }
      case "list_standup_dates": {
        const since = String(args.since);
        const until = String(args.until);
        return Object.keys(state.standups)
          .filter((date) => date >= since && date <= until)
          .sort();
      }
      case "add_manual_item": {
        const date = String(args.date);
        const file = state.standups[date];
        if (file === undefined) reject("not_found", `no standup file for ${date}`);
        const item = String(args.item);
        // Mirrors `fileops::add_manual`: appended verbatim, never rewritten.
        file.manual_region =
          file.manual_region.length === 0
            ? item
            : `${file.manual_region}\n${item}`;
        return null;
      }

      case "list_audit_sidecars":
        return state.sidecars[String(args.date)] ?? [];
      case "read_audit_sidecar": {
        const path = String(args.path);
        const data = state.auditData[path];
        if (data === undefined) reject("not_found", `no audit sidecar at ${path}`);
        return data;
      }

      case "get_pipeline_status":
        return state.pipelineStatus;
      case "get_filing_target":
        return makeFilingTarget(
          args.date === null || args.date === undefined
            ? toIso(new Date())
            : String(args.date),
        );
      case "preview_gather":
        return state.gatherPreview;

      case "get_scheduler_status":
        return state.schedulerStatus;
      case "set_scheduler_schedule": {
        const cron = String(args.cron);
        state.schedulerStatus.cron = cron;
        state.config.scheduler.cron = cron;
        return null;
      }
      case "set_scheduler_enabled":
        state.config.scheduler.enabled = Boolean(args.enabled);
        return null;

      case "discover_repos":
        return state.repos;
      case "get_standup_readiness": {
        // Derived from the same state the real backend derives it from, so a
        // spec that empties `repos` or `standup_authors` gets a coherent report.
        const configured = state.config.standup_authors
          .map((author) => author.trim())
          .filter((author) => author.length > 0);
        const identity = "tester@example.invalid";
        const effective = configured.length > 0 ? configured : [identity];
        return {
          github_dir: state.settingsPaths.github_dir,
          github_dir_exists: true,
          repo_count: state.repos.length,
          configured_authors: configured,
          git_identity: identity,
          effective_authors: effective,
          author_source: configured.length > 0 ? "configured" : "git-identity",
          ready: state.repos.length > 0,
        };
      }
      case "get_settings_paths":
        return state.settingsPaths;
      case "validate_paths":
        return state.pathValidations;
      case "open_in_file_manager":
        return null;
      case "detect_cloud_folders":
        return state.cloudFolders;
      case "configure_cloud_sync":
        return {
          root_path: String(args.rootPath),
          dailies_path: `${String(args.rootPath)}/autostand`,
          created: true,
        };
      case "get_repo_sync_status":
      case "setup_repo_sync":
        return {
          git_available: true,
          gh_available: true,
          gh_authenticated: true,
          can_setup: true,
          configured: false,
          enabled: false,
          repo_path: state.config.dailies_dir,
          repository: null,
          private: null,
          message: null,
        };

      case "get_dependency_status": {
        const group = args.group === null ? null : String(args.group);
        return group === null
          ? state.dependencies
          : state.dependencies.filter((entry) => entry.group === group);
      }
      case "run_dependency_remediation": {
        const id = String(args.dependencyId);
        const dependency = state.dependencies.find((entry) => entry.id === id);
        if (dependency === undefined) reject("not_found", `unknown dependency: ${id}`);
        // The backend re-probes after acting; the mock reports the same state
        // back so a spec has to drive `patchState` to simulate a fix.
        return {
          dependency_id: id,
          performed: dependency.remediation?.kind !== "in_app_action",
          message: `remediation for ${dependency.label}`,
          dependency,
        };
      }

      case "store_api_key": {
        const provider = findProvider(String(args.provider));
        if (provider === undefined) {
          reject("not_found", `unknown provider: ${String(args.provider)}`);
        }
        provider.api_key = { set: true, mode: "keychain" };
        return null;
      }
      case "get_api_key_status": {
        const provider = findProvider(String(args.provider));
        return provider?.api_key ?? { set: false, mode: "none" };
      }
      case "detect_cli": {
        const provider = findProvider(String(args.provider));
        return provider?.cli ?? { found: false, path: "", version: "" };
      }
      case "get_notification_status":
        return { supported: true, permission: "granted", config: state.config.notifications };
      case "request_notification_permission":
        return "granted";
      case "send_test_notification":
        return true;
      case "get_system_access":
      case "request_system_access":
        return state.systemAccess;
      case "open_access_settings":
        return null;
      case "list_local_models":
        return state.localModels;
      case "download_local_model":
      case "cancel_local_model_download":
      case "delete_local_model":
      case "select_local_model":
      case "accept_local_model_terms":
        return null;
      case "unload_local_models":
        return { processes_terminated: 0, caches_removed: 0, bytes_freed: 0 };

      default:
        reject("e2e_unstubbed", `invoke called with unstubbed command: ${command}`);
    }
  }

  mockIPC(
    (command, rawArgs) => {
      const args = rawArgs ?? {};
      calls.push({ command, args });

      const injected = scenario.errors[command];
      if (injected !== undefined) return Promise.reject(injected);

      if (scenario.defer.includes(command)) {
        return new Promise<unknown>((resolve, rejectPromise) => {
          pending.set(command, { resolve, reject: rejectPromise });
        });
      }

      return dispatch(command, args);
    },
    { shouldMockEvents: true },
  );

  /** Steps are named `gather*` / `render*`, same rule the frontend applies. */
  function stateForStep(step: string): BackendState["pipelineStatus"]["state"] {
    return step.startsWith("render") ? "rendering" : "gathering";
  }

  /**
   * Keep `get_pipeline_status` consistent with the events we push, the way the
   * real backend does. Without this a cache invalidation would refetch the
   * seeded idle status and undo the progress the spec just drove.
   */
  function mirrorEventIntoStatus(name: string, payload: unknown): void {
    const status = state.pipelineStatus;
    if (name === "pipeline-started") {
      const started = payload as PipelineStartedEvent;
      state.pipelineStatus = {
        ...status,
        state: "gathering",
        current_date: started.date,
        current_host: started.host,
        step: null,
        percent: 0,
        error: null,
      };
    } else if (name === "pipeline-progress") {
      const progress = payload as PipelineProgressEvent;
      state.pipelineStatus = {
        ...status,
        state: stateForStep(progress.step),
        current_date: progress.date,
        current_host: progress.host,
        step: progress.step,
        percent: progress.percent,
        error: null,
      };
    } else if (name === "pipeline-done") {
      const result = payload as PipelineDoneEvent;
      state.pipelineStatus = {
        ...status,
        state: result.status === "error" ? "error" : "done",
        current_date: result.date,
        current_host: result.host,
        step: null,
        percent: 100,
        last_run_at: new Date().toISOString(),
        last_result: result,
        error: result.status === "error" ? result.message : null,
      };
    } else if (name === "pipeline-error") {
      const failure = payload as PipelineErrorEvent;
      state.pipelineStatus = {
        ...status,
        state: "error",
        current_date: failure.date,
        step: failure.step,
        error: failure.message,
      };
    } else if (name === "run-started") {
      // A regeneration never emits `pipeline-started`/`pipeline-done` — it calls
      // `compile_one` directly — so its own bracket is what opens and closes the
      // status. `pipeline: false` runs (a provider test, a model download) must
      // leave the dashboard's progress bar alone, the same rule the frontend
      // applies in `usePipelineEvents`.
      const started = payload as RunStartedEvent;
      if (!started.pipeline) return;
      state.pipelineStatus = {
        ...status,
        state: "gathering",
        current_date: started.date,
        current_host: started.host,
        step: null,
        percent: 0,
        error: null,
      };
    } else if (name === "run-finished") {
      const finished = payload as RunFinishedEvent;
      if (!finished.pipeline) return;
      state.pipelineStatus = {
        ...status,
        state: finished.ok ? "done" : "error",
        step: null,
        percent: 100,
        last_run_at: new Date().toISOString(),
        error: finished.ok ? null : finished.message,
      };
    }
  }

  const bridge: E2EBridge = {
    state,
    calls,
    emit(name, payload) {
      mirrorEventIntoStatus(name, payload);
      void host.__TAURI_INTERNALS__?.invoke("plugin:event|emit", {
        event: name,
        payload,
      });
    },
    settle(command, result) {
      const deferred = pending.get(command);
      if (deferred === undefined) return false;
      pending.delete(command);
      deferred.resolve(result);
      return true;
    },
    fail(command, error) {
      const deferred = pending.get(command);
      if (deferred === undefined) return false;
      pending.delete(command);
      deferred.reject(error);
      return true;
    },
    isPending(command) {
      return pending.has(command);
    },
    callsTo(command) {
      return calls
        .filter((call) => call.command === command)
        .map((call) => call.args);
    },
    patchState(patch) {
      Object.assign(state, patch);
    },
  };

  host.__E2E__ = bridge;
}
