/**
 * Scheduler status and cron editing.
 *
 * The status reports which backend owns the schedule (launchd / systemd /
 * task-scheduler / in-process) plus the next and last run.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { configKey } from "@/hooks/use-config";
import { handleInvokeError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";

export const schedulerStatusKey = ["scheduler-status"] as const;

export function useSchedulerStatus() {
  return useQuery({
    queryKey: schedulerStatusKey,
    queryFn: tauriApi.getSchedulerStatus,
  });
}

export function useSetSchedule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (cron: string) => tauriApi.setSchedulerSchedule(cron),
    onSuccess: async (_data, cron) => {
      toast.success(`Schedule set to ${cron}`);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: schedulerStatusKey }),
        // `scheduler.cron` is persisted inside the app config as well.
        queryClient.invalidateQueries({ queryKey: configKey }),
      ]);
    },
    onError: (error) => handleInvokeError(error, "Set schedule"),
  });
}

/** Alias spelled after the `set_scheduler_schedule` command. */
export const useSetSchedulerSchedule = useSetSchedule;
