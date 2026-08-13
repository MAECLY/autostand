import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { handleInvokeError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";

export const notificationStatusKey = ["notification-status"] as const;

export function useNotificationStatus() {
  return useQuery({
    queryKey: notificationStatusKey,
    queryFn: tauriApi.getNotificationStatus,
  });
}

export function useRequestNotificationPermission() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: tauriApi.requestNotificationPermission,
    onSuccess: async (permission) => {
      toast.success(`Notification permission: ${permission}`);
      await queryClient.invalidateQueries({ queryKey: notificationStatusKey });
    },
    onError: (error) =>
      handleInvokeError(error, "Request notification permission"),
  });
}

export function useSendTestNotification() {
  return useMutation({
    mutationFn: tauriApi.sendTestNotification,
    onSuccess: (sent) => {
      if (sent) toast.success("Test notification sent");
      else toast.warning("Test notification was suppressed by your settings");
    },
    onError: (error) => handleInvokeError(error, "Send test notification"),
  });
}
