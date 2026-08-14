/**
 * Feature prerequisites and their guided remediation.
 *
 * Every fetch spawns child processes on the Rust side (`gh auth status`, PATH
 * walks, filesystem probes), so the query is deliberately slow-moving: it does
 * not refetch on mount or on window focus, and the checklist offers an explicit
 * Recheck instead. Both settings tabs share one query per group, so mounting the
 * checklist next to a consumer of the same data costs nothing extra.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { localModelsKey } from "@/hooks/use-local-models";
import { repoSyncStatusKey } from "@/hooks/use-sync";
import { handleInvokeError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";
import type { Dependency, DependencyGroup } from "@/lib/types";

export const dependenciesKey = ["dependencies"] as const;

/** Stable ids, mirroring the constants in `commands::dependencies`. */
export const DEPENDENCY_IDS = {
  git: "repo-sync.git",
  gh: "repo-sync.gh",
  ghAuth: "repo-sync.gh-auth",
  sidecar: "local-ai.sidecar",
  runtime: "local-ai.runtime",
  model: "local-ai.model",
} as const;

export function dependencyGroupKey(group: DependencyGroup) {
  return [...dependenciesKey, group] as const;
}

/** Long enough that switching tabs never re-probes; Recheck is the escape. */
export const DEPENDENCY_STALE_TIME_MS = 5 * 60 * 1000;

export function useDependencies(group: DependencyGroup) {
  return useQuery({
    queryKey: dependencyGroupKey(group),
    queryFn: () => tauriApi.getDependencyStatus(group),
    staleTime: DEPENDENCY_STALE_TIME_MS,
    refetchOnMount: false,
    refetchOnWindowFocus: false,
  });
}

/** Find one dependency by id, tolerating a query that has not resolved yet. */
export function findDependency(
  dependencies: Dependency[] | undefined,
  id: string,
): Dependency | undefined {
  return dependencies?.find((dependency) => dependency.id === id);
}

export function useRunDependencyRemediation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: tauriApi.runDependencyRemediation,
    onSuccess: async (outcome) => {
      // `performed: false` means the step is the user's to take — saying
      // "done" there would be a lie the next probe immediately contradicts.
      if (outcome.performed) toast.success(outcome.message);
      else toast.info(outcome.message);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: dependenciesKey }),
        queryClient.invalidateQueries({ queryKey: repoSyncStatusKey }),
        queryClient.invalidateQueries({ queryKey: localModelsKey }),
      ]);
    },
    onError: (error) => handleInvokeError(error, "Fix requirement"),
  });
}
