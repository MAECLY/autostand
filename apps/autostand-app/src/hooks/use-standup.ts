/**
 * Standup file reads plus the MANUAL-region append.
 *
 * Keys are `["standup", date]` — the same shape the pipeline events and the
 * compile mutations invalidate after a run.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { handleInvokeError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";
import type { StandupFileContent } from "@/lib/types";

export function standupKey(date: string) {
  return ["standup", date] as const;
}

export function useStandupFile(date: string) {
  return useQuery({
    queryKey: standupKey(date),
    queryFn: () => tauriApi.readStandupFile(date),
    enabled: date.length > 0,
  });
}

export interface AddManualItemVariables {
  /** Filing date, `YYYY-MM-DD`. */
  date: string;
  /** Verbatim line appended to the MANUAL region. */
  item: string;
}

/**
 * Mirrors `fileops::add_manual`: the item is appended verbatim, newline-joined,
 * with no bullet prefix — so the optimistic body matches what disk will hold.
 */
function appendManualItem(region: string, item: string): string {
  return region.length === 0 ? item : `${region}\n${item}`;
}

export function useAddManualItem() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ date, item }: AddManualItemVariables) =>
      tauriApi.addManualItem(date, item),
    onMutate: async ({ date, item }) => {
      const key = standupKey(date);
      // Stop an in-flight read from overwriting the optimistic body.
      await queryClient.cancelQueries({ queryKey: key });
      const previous = queryClient.getQueryData<StandupFileContent>(key);

      queryClient.setQueryData<StandupFileContent>(key, (file) =>
        file
          ? { ...file, manual_region: appendManualItem(file.manual_region, item) }
          : file,
      );

      return { key, previous };
    },
    onError: (error, _variables, context) => {
      if (context?.previous) {
        queryClient.setQueryData(context.key, context.previous);
      }
      handleInvokeError(error, "Add manual item");
    },
    onSettled: (_data, _error, { date }) =>
      queryClient.invalidateQueries({ queryKey: standupKey(date) }),
  });
}
