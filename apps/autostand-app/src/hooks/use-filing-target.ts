/**
 * Which standup file a day of work is destined for.
 *
 * The answer comes from the backend (`get_filing_target`), never from date
 * arithmetic in the UI: the filing rule decides which file the pipeline writes,
 * so a second implementation here could announce a file no compile touches.
 *
 * The answer depends on `AppConfig.dates.archive_mode`, which the backend reads
 * from the store itself. That dependency is invisible to React Query, so
 * `useSetConfig` invalidates {@link filingTargetKey} — it is the single writer
 * of `AppConfig`, and without that line saving a new policy would leave the
 * dashboard announcing the file the old one produced.
 */

import { useQuery } from "@tanstack/react-query";

import { tauriApi } from "@/lib/tauri";

export const filingTargetKey = ["filing-target"] as const;

/** Resolve the filing target for `workDay` (default: today). */
export function useFilingTarget(workDay?: string) {
  return useQuery({
    queryKey: [...filingTargetKey, workDay ?? null],
    queryFn: () => tauriApi.getFilingTarget(workDay),
  });
}
