import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { cloudFoldersKey } from "@/hooks/use-cloud-folders";
import { configKey } from "@/hooks/use-config";
import { handleInvokeError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";

export const repoSyncStatusKey = ["repo-sync-status"] as const;

export function useConfigureCloudSync() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: tauriApi.configureCloudSync,
    onSuccess: async (selection) => {
      toast.success("Cloud Sync configured", {
        description: `Standups will be saved in ${selection.dailies_path}`,
      });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: configKey }),
        queryClient.invalidateQueries({ queryKey: cloudFoldersKey }),
        queryClient.invalidateQueries({ queryKey: repoSyncStatusKey }),
      ]);
    },
    onError: (error) => handleInvokeError(error, "Configure Cloud Sync"),
  });
}

export function useRepoSyncStatus() {
  return useQuery({
    queryKey: repoSyncStatusKey,
    queryFn: tauriApi.getRepoSyncStatus,
  });
}

export function useSetupRepoSync() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (repoName: string) => tauriApi.setupRepoSync(repoName),
    onSuccess: async (status) => {
      toast.success("Private Repo Sync configured", {
        description: status.repository ?? "GitHub repository ready",
      });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: configKey }),
        queryClient.invalidateQueries({ queryKey: repoSyncStatusKey }),
      ]);
    },
    onError: (error) => handleInvokeError(error, "Set up Repo Sync"),
  });
}
