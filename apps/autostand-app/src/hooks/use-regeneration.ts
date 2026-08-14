import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { invalidateCompiled } from "@/hooks/use-pipeline-status";
import { handleInvokeError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";
import type { RegenerationResolution } from "@/lib/types";

export function usePreviewRegeneration() {
  return useMutation({
    mutationFn: (date: string) => tauriApi.previewRegeneration(date),
    onError: (error) => handleInvokeError(error, "Generate standup comparison"),
  });
}

export function useApplyRegeneration() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      token,
      resolution,
      mergedAuto,
    }: {
      token: string;
      resolution: RegenerationResolution;
      mergedAuto?: string;
    }) => tauriApi.applyRegeneration(token, resolution, mergedAuto),
    onSuccess: async (result) => {
      toast.success(
        result.resolution === "keep_current" ? "Current standup kept" : "Standup regenerated",
        { description: result.message },
      );
      await invalidateCompiled(queryClient, result.date);
    },
    onError: (error) => handleInvokeError(error, "Apply standup comparison"),
  });
}
