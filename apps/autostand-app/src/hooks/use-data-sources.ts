/**
 * Data source toggles.
 *
 * `local-git` is authoritative and always enabled; the backend rejects turning
 * it off, and the optimistic update rolls back when it does.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { configKey } from "@/hooks/use-config";
import { handleInvokeError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";
import type { DataSourceConfig } from "@/lib/types";

export const dataSourcesKey = ["data-sources"] as const;

export function useDataSources() {
  return useQuery({
    queryKey: dataSourcesKey,
    queryFn: tauriApi.listDataSources,
  });
}

export interface ToggleDataSourceVariables {
  /** Stable source id, e.g. `local-git`. */
  id: string;
  enabled: boolean;
}

export function useToggleDataSource() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, enabled }: ToggleDataSourceVariables) =>
      tauriApi.toggleDataSource(id, enabled),
    onMutate: async ({ id, enabled }) => {
      // A switch must move on click; stop an in-flight list from undoing it.
      await queryClient.cancelQueries({ queryKey: dataSourcesKey });
      const previous =
        queryClient.getQueryData<DataSourceConfig[]>(dataSourcesKey);

      queryClient.setQueryData<DataSourceConfig[]>(dataSourcesKey, (sources) =>
        sources?.map((source) =>
          source.id === id ? { ...source, enabled } : source,
        ),
      );

      return { previous };
    },
    onError: (error, _variables, context) => {
      if (context?.previous) {
        queryClient.setQueryData(dataSourcesKey, context.previous);
      }
      handleInvokeError(error, "Toggle data source");
    },
    onSettled: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: dataSourcesKey }),
        // The toggles are persisted under `config.data_sources`.
        queryClient.invalidateQueries({ queryKey: configKey }),
      ]),
  });
}
