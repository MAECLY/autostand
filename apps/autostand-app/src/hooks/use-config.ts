/**
 * App configuration and host slug.
 *
 * `["config"]` backs every settings surface, so anything that mutates a field
 * of `AppConfig` — schedule, data sources, providers — invalidates it too.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { handleInvokeError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";
import type { AppConfig } from "@/lib/types";

export const configKey = ["config"] as const;
export const hostSlugKey = ["host-slug"] as const;

export function useConfig() {
  return useQuery({
    queryKey: configKey,
    queryFn: tauriApi.getConfig,
  });
}

export function useSetConfig() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (config: AppConfig) => tauriApi.setConfig(config),
    onSuccess: () => {
      toast.success("Settings saved");
      return queryClient.invalidateQueries({ queryKey: configKey });
    },
    onError: (error) => handleInvokeError(error, "Save settings"),
  });
}

export function useHostSlug() {
  return useQuery({
    queryKey: hostSlugKey,
    queryFn: tauriApi.getHostSlug,
  });
}

export function useSetHostSlug() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (slug: string) => tauriApi.setHostSlug(slug),
    onSuccess: async (_data, slug) => {
      toast.success(`Host slug set to ${slug}`);
      // The slug is mirrored into `host_slug_override`, so config is stale too.
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: hostSlugKey }),
        queryClient.invalidateQueries({ queryKey: configKey }),
      ]);
    },
    onError: (error) => handleInvokeError(error, "Set host slug"),
  });
}
