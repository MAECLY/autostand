/**
 * Filing dates that already have a `YYYY-MM-DD.md` on disk.
 *
 * The backend answers with one `read_dir`; the UI still reads individual
 * files through `standupKey(date)` so a compile invalidates both layers.
 */

import { useQuery } from "@tanstack/react-query";

import { tauriApi } from "@/lib/tauri";

export function standupDatesKey(since: string, until: string) {
  return ["standup-dates", since, until] as const;
}

export function useStandupDatesInRange(since: string, until: string) {
  return useQuery({
    queryKey: standupDatesKey(since, until),
    queryFn: () => tauriApi.listStandupDates(since, until),
    enabled: since.length > 0 && until.length > 0,
  });
}
