/**
 * Pipeline status: one poll-free query kept warm by the backend events.
 *
 * `usePipelineStatus` reads `get_pipeline_status`; `usePipelineEvents` (mount
 * once, near the app shell) pushes `pipeline-*` payloads into the same key so
 * progress bars update without polling. It also owns the shared post-run
 * bookkeeping (`announceCompileResult`, `invalidateCompiled`) that the compile
 * mutations reuse — keeping the dependency one-way avoids an import cycle.
 */

import { useEffect, useRef } from "react";
import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { auditSidecarsKey } from "@/hooks/use-audit";
import {
  clearPipelineLog,
  pushPipelineLog,
} from "@/hooks/use-pipeline-log";
import { schedulerStatusKey } from "@/hooks/use-scheduler";
import { standupKey } from "@/hooks/use-standup";
import { handleInvokeError } from "@/lib/error";
import { useUiStore } from "@/lib/store";
import {
  onPipelineDone,
  onPipelineError,
  onPipelineLog,
  onPipelineProgress,
  onPipelineStarted,
  onSchedulerTick,
  tauriApi,
} from "@/lib/tauri";
import type { CompileResult, PipelineStatus, TriggerSource } from "@/lib/types";

export const pipelineStatusKey = ["pipeline-status"] as const;

/** Unsubscribe handle returned by the `on*` helpers, without importing Tauri types. */
type Unsubscribe = Awaited<ReturnType<typeof onPipelineDone>>;

const IDLE_STATUS: PipelineStatus = {
  state: "idle",
  current_date: null,
  current_host: null,
  step: null,
  percent: 0,
  last_run_at: null,
  last_result: null,
  error: null,
};

/** Toast a finished run. Shared so scheduled and manual runs read identically. */
export function announceCompileResult(result: CompileResult): void {
  if (result.status === "error") {
    toast.error(`Compile failed — ${result.date}`, {
      description: result.message,
    });
    return;
  }
  if (result.status === "skip") {
    toast.info(`Skipped — ${result.date}`, { description: result.message });
    return;
  }
  toast.success(`Standup compiled — ${result.date}`, {
    description: result.message,
  });
}

/** Drop every cache entry a completed run invalidates. */
export function invalidateCompiled(
  queryClient: QueryClient,
  date: string,
): Promise<void> {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: standupKey(date) }),
    queryClient.invalidateQueries({ queryKey: ["standup-dates"] }),
    queryClient.invalidateQueries({ queryKey: auditSidecarsKey(date) }),
    queryClient.invalidateQueries({ queryKey: pipelineStatusKey }),
  ]).then(() => undefined);
}

export function usePipelineStatus() {
  return useQuery({
    queryKey: pipelineStatusKey,
    queryFn: tauriApi.getPipelineStatus,
    // Events keep this key fresh; re-read on mount so a run started while the
    // view was unmounted still shows up.
    staleTime: 0,
  });
}

/** Steps are named `gather*` / `render*`; anything else is still gathering. */
function stateForStep(step: string): PipelineStatus["state"] {
  return step.startsWith("render") ? "rendering" : "gathering";
}

/**
 * Subscribe to the backend pipeline events for as long as the caller is mounted.
 * Mount this exactly once (root layout) — every extra mount duplicates toasts.
 */
export function usePipelineEvents(): void {
  const queryClient = useQueryClient();
  // Manual runs are announced by the compile mutation that started them, so the
  // `pipeline-done` handler only toasts runs the UI did not initiate.
  const trigger = useRef<TriggerSource | null>(null);

  useEffect(() => {
    // StrictMode mounts twice: cleanup can run before `listen` resolves, so a
    // late subscription unsubscribes itself instead of leaking.
    let disposed = false;
    const unsubscribes: Unsubscribe[] = [];

    const track = (pending: Promise<Unsubscribe>): void => {
      pending
        .then((unsubscribe) => {
          if (disposed) unsubscribe();
          else unsubscribes.push(unsubscribe);
        })
        .catch((error: unknown) => {
          handleInvokeError(error, "Pipeline events");
        });
    };

    track(
      onPipelineStarted((payload) => {
        trigger.current = payload.trigger;
        clearPipelineLog();
        useUiStore.getState().setTerminalPanel("open");
        queryClient.setQueryData<PipelineStatus>(pipelineStatusKey, (status) => ({
          ...(status ?? IDLE_STATUS),
          state: "gathering",
          current_date: payload.date,
          current_host: payload.host,
          step: null,
          percent: 0,
          error: null,
        }));
      }),
    );

    track(
      onPipelineLog((line) => {
        pushPipelineLog(line);
      }),
    );

    track(
      onPipelineProgress((payload) => {
        queryClient.setQueryData<PipelineStatus>(pipelineStatusKey, (status) => ({
          ...(status ?? IDLE_STATUS),
          state: stateForStep(payload.step),
          current_date: payload.date,
          current_host: payload.host,
          step: payload.step,
          percent: payload.percent,
          error: null,
        }));
      }),
    );

    track(
      onPipelineDone((result) => {
        queryClient.setQueryData<PipelineStatus>(pipelineStatusKey, (status) => ({
          ...(status ?? IDLE_STATUS),
          state: result.status === "error" ? "error" : "done",
          current_date: result.date,
          current_host: result.host,
          step: null,
          percent: 100,
          last_run_at: new Date().toISOString(),
          last_result: result,
          error: result.status === "error" ? result.message : null,
        }));

        if (trigger.current !== "manual") announceCompileResult(result);
        trigger.current = null;

        void invalidateCompiled(queryClient, result.date);
      }),
    );

    track(
      onPipelineError((payload) => {
        queryClient.setQueryData<PipelineStatus>(pipelineStatusKey, (status) => ({
          ...(status ?? IDLE_STATUS),
          state: "error",
          current_date: payload.date,
          step: payload.step,
          error: payload.message,
        }));
        // Same rule as `pipeline-done`: a manual run is reported once, by the
        // mutation that started it. `trigger` is left set — a `pipeline-done`
        // may still follow this error.
        if (trigger.current !== "manual") {
          toast.error(`${payload.step} — ${payload.code}`, {
            description: payload.message,
          });
        }
      }),
    );

    track(
      onSchedulerTick(() => {
        void queryClient.invalidateQueries({ queryKey: schedulerStatusKey });
      }),
    );

    return () => {
      disposed = true;
      for (const unsubscribe of unsubscribes) unsubscribe();
      unsubscribes.length = 0;
    };
  }, [queryClient]);
}
