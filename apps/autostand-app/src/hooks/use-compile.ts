/**
 * Compile mutations — the only way the UI starts a pipeline run.
 *
 * The backend also emits `pipeline-*` events for these runs; `usePipelineEvents`
 * defers the toast to the mutation when the trigger is manual, so a click
 * produces exactly one notification.
 */

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import {
  announceCompileResult,
  invalidateCompiled,
} from "@/hooks/use-pipeline-status";
import { handleInvokeError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";

/**
 * Compile one filing date. `void` in the union — rather than `string | undefined`
 * — is what lets callers write `mutate()` to mean "today".
 */
export function useCompileStandup() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (date: string | void) =>
      tauriApi.compileStandup(date ?? undefined),
    onSuccess: (result) => {
      announceCompileResult(result);
      return invalidateCompiled(queryClient, result.date);
    },
    onError: (error) => handleInvokeError(error, "Compile standup"),
  });
}

/** Compile every pending filing date (self-heal for missed runs). */
export function useCompileAll() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: tauriApi.compileAll,
    onSuccess: async (results) => {
      // One summary toast: a self-heal can cover a week of missed days.
      const failed = results.filter((r) => r.status === "error").length;
      if (failed > 0) {
        toast.error(`${failed} of ${results.length} runs failed`);
      } else {
        toast.success(
          `Compiled ${results.length} standup${results.length === 1 ? "" : "s"}`,
        );
      }
      await Promise.all(
        results.map((result) => invalidateCompiled(queryClient, result.date)),
      );
    },
    onError: (error) => handleInvokeError(error, "Compile all"),
  });
}

/** Run the scheduled job now, for the date the scheduler would have picked. */
export function useTriggerRunNow() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: tauriApi.triggerRunNow,
    onSuccess: (result) => {
      announceCompileResult(result);
      return invalidateCompiled(queryClient, result.date);
    },
    onError: (error) => handleInvokeError(error, "Run now"),
  });
}
