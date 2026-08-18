/**
 * What the OS lets the app read, and the one action that can change it.
 *
 * Kept separate from {@link useStandupReadiness}: an unreadable folder and an
 * empty one produce the same symptom — no facts — and the fixes are opposite.
 * One sends the user to System Settings, the other to Settings → Paths.
 *
 * The query runs on mount and is not refetched on focus. macOS answers from its
 * own record once a decision is made, so the answer only changes when the user
 * leaves the app to change it — which is exactly when the mutation below runs
 * and invalidates this.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { tauriApi } from "@/lib/tauri";

export const systemAccessKey = ["system-access"] as const;

export function useSystemAccess() {
  return useQuery({
    queryKey: systemAccessKey,
    queryFn: tauriApi.getSystemAccess,
    refetchOnWindowFocus: false,
  });
}

/**
 * Ask the OS for access, then replace the cached answer with what it said.
 *
 * `setQueryData` rather than an invalidate: the command returns the fresh status
 * itself, and re-probing would raise a second round of consent dialogs.
 */
export function useRequestSystemAccess() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: tauriApi.requestSystemAccess,
    onSuccess: (access) => queryClient.setQueryData(systemAccessKey, access),
  });
}
