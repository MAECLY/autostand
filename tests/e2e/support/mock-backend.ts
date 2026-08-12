/**
 * The fake autostand backend, as it exists inside the browser page.
 *
 * `installMockBackend` is serialised by `page.addInitScript` and evaluated
 * before any application script runs, so it must be entirely self-contained:
 * every helper lives inside the function body and the only imports are
 * type-only (erased at transpile time).
 *
 * It routes `invoke` through the real `mockIPC` from `@tauri-apps/api/mocks`
 * — loaded by the preceding init script — and answers each of the 25 IPC
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
      case "test_llm_provider": {
        const id = String(args.provider);
        const result = state.providerTests[id];
        if (result === undefined) {
          reject("not_found", `no scripted test result for provider: ${id}`);
        }
        return result;
      }

      case "compile_standup":
      case "trigger_run_now":
        return state.compileResult;
      case "compile_all":
        return [state.compileResult];

      case "read_standup_file": {
        const date = String(args.date);
        const file = state.standups[date];
        // The dashboard's empty state keys off exactly this code.
        if (file === undefined) reject("not_found", `no standup file for ${date}`);
        return file;
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

      case "discover_repos":
        return state.repos;
      case "get_settings_paths":
        return state.settingsPaths;
      case "validate_paths":
        return state.pathValidations;
      case "detect_cloud_folders":
        return state.cloudFolders;

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
