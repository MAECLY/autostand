import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { handleInvokeError } from "@/lib/error";
import { onLocalModelProgress, tauriApi } from "@/lib/tauri";
import type { LocalModelInfo } from "@/lib/types";

export const localModelsKey = ["local-models"] as const;

export function useLocalModels() {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: localModelsKey,
    queryFn: tauriApi.listLocalModels,
  });

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void onLocalModelProgress((progress) => {
      queryClient.setQueryData<LocalModelInfo[]>(localModelsKey, (models) =>
        models?.map((model) =>
          model.id === progress.model_id
            ? {
                ...model,
                status: progress.status,
                downloaded_bytes: progress.downloaded_bytes,
                error: progress.error,
              }
            : model,
        ),
      );
      if (progress.status === "available") {
        toast.success("Local model ready");
      } else if (progress.status === "error") {
        toast.error("Local model download failed", {
          description: progress.error ?? undefined,
        });
      }
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [queryClient]);

  return query;
}

function useModelMutation(
  action: (modelId: string) => Promise<void>,
  actionLabel: string,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: action,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: localModelsKey });
    },
    onError: (error) => handleInvokeError(error, actionLabel),
  });
}

export function useDownloadLocalModel() {
  return useModelMutation(tauriApi.downloadLocalModel, "Download local model");
}

export function useCancelLocalModelDownload() {
  return useModelMutation(
    tauriApi.cancelLocalModelDownload,
    "Cancel local model download",
  );
}

export function useDeleteLocalModel() {
  return useModelMutation(tauriApi.deleteLocalModel, "Delete local model");
}

export function useSelectLocalModel() {
  return useModelMutation(tauriApi.selectLocalModel, "Select local model");
}

export function useAcceptLocalModelTerms() {
  return useModelMutation(
    tauriApi.acceptLocalModelTerms,
    "Accept local model terms",
  );
}
