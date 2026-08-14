/**
 * Filesystem paths the app reads from, their validation verdicts, and repo
 * discovery under `github_dir`.
 *
 * Validation and discovery are mutations, not queries: both walk the disk and
 * must run when the user asks (blur, button) rather than on mount.
 */

import { useMutation, useQuery } from "@tanstack/react-query";

import { handleInvokeError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";

export const settingsPathsKey = ["settings-paths"] as const;

export function useSettingsPaths() {
  return useQuery({
    queryKey: settingsPathsKey,
    queryFn: tauriApi.getSettingsPaths,
  });
}

/** Re-check every configured path. Resolves to one `PathValidation` per key. */
export function useValidatePaths() {
  return useMutation({
    mutationFn: tauriApi.validatePaths,
    onError: (error) => handleInvokeError(error, "Validate paths"),
  });
}

/**
 * Hand a directory to the OS file manager (Finder / Explorer / `xdg-open`).
 *
 * A mutation, not a query: it launches an external application, so it may only
 * ever run from an explicit click.
 */
export function useOpenInFileManager() {
  return useMutation({
    mutationFn: tauriApi.openInFileManager,
    onError: (error) => handleInvokeError(error, "Open folder"),
  });
}

/** Scan `github_dir` for git repos. Expensive — keep it behind a button. */
export function useDiscoverRepos() {
  return useMutation({
    mutationFn: tauriApi.discoverRepos,
    onError: (error) => handleInvokeError(error, "Discover repos"),
  });
}
