import { Bell, Send } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import { Input } from "@autostand/ui/components/input";
import { Label } from "@autostand/ui/components/label";
import { Switch } from "@autostand/ui/components/switch";

import {
  useNotificationStatus,
  useRequestNotificationPermission,
  useSendTestNotification,
} from "@/hooks/use-notifications";
import { useConfig, useSetConfig } from "@/hooks/use-config";
import type { NotificationConfig } from "@/lib/types";

interface NotificationToggleProps {
  id: string;
  label: string;
  description: string;
  checked: boolean;
  disabled?: boolean;
  onCheckedChange: (checked: boolean) => void;
}

function NotificationToggle({
  id,
  label,
  description,
  checked,
  disabled,
  onCheckedChange,
}: NotificationToggleProps) {
  return (
    <div className="flex items-center justify-between gap-6 border-b border-border py-4 last:border-b-0">
      <div>
        <Label htmlFor={id}>{label}</Label>
        <p className="text-xs text-muted-foreground">{description}</p>
      </div>
      <Switch
        id={id}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onCheckedChange}
      />
    </div>
  );
}

export function NotificationsTab() {
  const config = useConfig();
  const setConfig = useSetConfig();
  const status = useNotificationStatus();
  const permission = useRequestNotificationPermission();
  const test = useSendTestNotification();

  const notifications = config.data?.notifications;
  function patch(changes: Partial<NotificationConfig>) {
    if (config.data === undefined || notifications === undefined) return;
    setConfig.mutate({
      ...config.data,
      notifications: { ...notifications, ...changes },
    });
  }

  if (config.isPending || notifications === undefined) {
    return <p className="text-sm text-muted-foreground">Loading notifications…</p>;
  }

  const permissionState = status.data?.permission ?? "unknown";
  const categoriesDisabled = !notifications.enabled;

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center justify-between gap-4 rounded-lg border border-border p-4">
        <div className="flex items-center gap-3">
          <Bell className="size-5 text-primary" aria-hidden="true" />
          <div>
            <p className="font-medium">System permission</p>
            <p className="text-xs text-muted-foreground">
              Native alerts work on macOS, Windows and Linux, including scheduled headless runs.
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Badge
            variant={permissionState === "granted" ? "success" : "secondary"}
          >
            {permissionState}
          </Badge>
          {permissionState !== "granted" ? (
            <Button
              type="button"
              variant="outline"
              disabled={permission.isPending}
              onClick={() => permission.mutate()}
            >
              Allow notifications
            </Button>
          ) : null}
        </div>
      </div>

      <NotificationToggle
        id="notifications-enabled"
        label="Enable system notifications"
        description="Master switch. Permission alone never enables alerts."
        checked={notifications.enabled}
        onCheckedChange={(enabled) => patch({ enabled })}
      />
      <NotificationToggle
        id="notifications-low-usage"
        label="Low AI usage"
        description="Alert only when the provider reports an exact remaining percentage."
        checked={notifications.low_usage}
        disabled={categoriesDisabled}
        onCheckedChange={(low_usage) => patch({ low_usage })}
      />

      <div className="flex items-center justify-between gap-6 border-b border-border py-4">
        <div>
          <Label htmlFor="low-usage-threshold">Low usage threshold</Label>
          <p className="text-xs text-muted-foreground">
            Notify at or below this remaining percentage.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Input
            id="low-usage-threshold"
            type="number"
            min={0}
            max={100}
            className="w-20"
            disabled={categoriesDisabled || !notifications.low_usage}
            value={notifications.low_usage_threshold_percent}
            onChange={(event) => {
              const value = Number.parseInt(event.target.value, 10);
              if (!Number.isNaN(value)) {
                patch({ low_usage_threshold_percent: Math.min(100, Math.max(0, value)) });
              }
            }}
          />
          <span className="text-sm text-muted-foreground">%</span>
        </div>
      </div>

      <NotificationToggle
        id="notifications-exhausted"
        label="Provider exhausted"
        description="Alert when usage or billing is exhausted."
        checked={notifications.provider_exhausted}
        disabled={categoriesDisabled}
        onCheckedChange={(provider_exhausted) => patch({ provider_exhausted })}
      />
      <NotificationToggle
        id="notifications-fallback"
        label="Provider fallback"
        description="Alert when Autostand continues with another AI provider."
        checked={notifications.provider_fallback}
        disabled={categoriesDisabled}
        onCheckedChange={(provider_fallback) => patch({ provider_fallback })}
      />
      <NotificationToggle
        id="notifications-models"
        label="Local model downloads"
        description="Alert when a model is ready or a download fails."
        checked={notifications.local_model_downloads}
        disabled={categoriesDisabled}
        onCheckedChange={(local_model_downloads) => patch({ local_model_downloads })}
      />
      <NotificationToggle
        id="notifications-standup-complete"
        label="Standup completed"
        description="Off by default to avoid a daily success notification."
        checked={notifications.standup_complete}
        disabled={categoriesDisabled}
        onCheckedChange={(standup_complete) => patch({ standup_complete })}
      />
      <NotificationToggle
        id="notifications-standup-failed"
        label="Standup failed"
        description="Alert when a scheduled or manual compile fails."
        checked={notifications.standup_failed}
        disabled={categoriesDisabled}
        onCheckedChange={(standup_failed) => patch({ standup_failed })}
      />

      <div className="flex justify-end">
        <Button
          type="button"
          variant="outline"
          disabled={
            !notifications.enabled ||
            permissionState !== "granted" ||
            test.isPending
          }
          onClick={() => test.mutate()}
        >
          <Send aria-hidden="true" />
          Send test notification
        </Button>
      </div>
    </div>
  );
}
