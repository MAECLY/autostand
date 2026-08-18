/**
 * Checking for, downloading and installing a new version from inside the app.
 *
 * Until 1.2.0 a fix reached anyone only if they noticed a GitHub release and
 * re-downloaded an installer, which for most people means never. The bundles are
 * signed with the updater key independently of code signing, so this works on
 * builds macOS and Windows still consider unsigned — and because the new bundle
 * is written by the app rather than downloaded by a browser, it arrives without
 * the quarantine flag that makes a fresh download need `xattr -rd`.
 *
 * Checking is manual on purpose. A background check is a network request the
 * user did not ask for, from an app whose entire pitch is that nothing leaves
 * the machine unless they point it somewhere.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { tauriUpdater, type Update } from "@/lib/tauri";

/** What the check found, flattened so the UI never holds the plugin's handle. */
export interface UpdateAvailability {
  available: boolean;
  version: string | null;
  /** Release notes as published, which is the CHANGELOG entry for that tag. */
  notes: string | null;
  date: string | null;
}

export const updateCheckKey = ["update-check"] as const;

/**
 * The pending update, kept outside React state.
 *
 * The plugin's `Update` carries a native handle and is not serializable, so it
 * cannot live in the query cache. Downloading needs the very object the check
 * returned — a second `check()` would produce a different handle.
 */
let pending: Update | null = null;

/**
 * Look for a newer release. Disabled until something asks for it.
 *
 * `enabled: false` plus `refetch` rather than a mutation: the result is state
 * the UI keeps showing ("you are up to date"), not an action's outcome.
 */
export function useUpdateCheck() {
  return useQuery({
    queryKey: updateCheckKey,
    enabled: false,
    retry: false,
    gcTime: 0,
    queryFn: async (): Promise<UpdateAvailability> => {
      const update = await tauriUpdater.check();
      pending = update;
      if (update === null) {
        return { available: false, version: null, notes: null, date: null };
      }
      return {
        available: true,
        version: update.version,
        notes: update.body ?? null,
        date: update.date ?? null,
      };
    },
  });
}

/** Progress of the download, as a fraction, or `null` before it starts. */
export type DownloadProgress = number | null;

export function useInstallUpdate() {
  const queryClient = useQueryClient();
  const [progress, setProgress] = useState<DownloadProgress>(null);

  const mutation = useMutation({
    mutationFn: async () => {
      if (pending === null) {
        throw new Error("Check for an update before installing one.");
      }
      let downloaded = 0;
      let total = 0;
      await pending.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
          setProgress(0);
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          // A server that sends no Content-Length leaves the bar indeterminate
          // rather than inventing a denominator.
          setProgress(total > 0 ? downloaded / total : null);
        } else if (event.event === "Finished") {
          setProgress(1);
        }
      });
    },
    onSuccess: () => {
      // The installed version is the answer to the next check, and the old one
      // is now wrong on screen.
      queryClient.removeQueries({ queryKey: updateCheckKey });
    },
  });

  return { ...mutation, progress, relaunch: tauriUpdater.relaunch };
}
