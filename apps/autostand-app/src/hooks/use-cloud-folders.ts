/**
 * Cloud-sync folder detection for the multi-device standup sync feature.
 *
 * The probe walks the filesystem (well-known iCloud / OneDrive / Syncthing /
 * Nextcloud / Dropbox paths), so it runs as a query. Re-fetching is cheap and
 * safe — call `invalidate` after the user (re)enables a cloud client.
 */

import { useQuery, useQueryClient } from "@tanstack/react-query";

import { tauriApi } from "@/lib/tauri";

export const cloudFoldersKey = ["cloud-folders"] as const;

export function useCloudFolders() {
  return useQuery({
    queryKey: cloudFoldersKey,
    queryFn: tauriApi.detectCloudFolders,
  });
}

export function useInvalidateCloudFolders() {
  const queryClient = useQueryClient();
  return () => queryClient.invalidateQueries({ queryKey: cloudFoldersKey });
}