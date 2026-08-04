/**
 * Settings → Scheduler: cron editor plus the read-only view of what the
 * installed scheduler is actually doing.
 *
 * The cron string is validated in the browser against the same 5-field POSIX
 * subset `crates/autostand-scheduler/src/cron.rs` accepts, so an unparseable
 * expression never reaches `set_scheduler_schedule` (which would reinstall a
 * broken system unit).
 */

import { useState, type ReactNode } from "react";

import { Badge } from "@autostand/ui/components/badge";
import { Input } from "@autostand/ui/components/input";
import { Label } from "@autostand/ui/components/label";
import { Switch } from "@autostand/ui/components/switch";

import { useConfig, useSetConfig } from "@/hooks/use-config";
import {
  useSchedulerStatus,
  useSetSchedulerSchedule,
} from "@/hooks/use-scheduler";
import type {
  SchedulerConfig,
  SchedulerSource,
  TriggerSource,
} from "@/lib/types";
import { formatIsoDate, formatRelative } from "@/lib/utils";

const SOURCE_LABELS: Record<SchedulerSource, string> = {
  launchd: "launchd (macOS)",
  systemd: "systemd (Linux)",
  "task-scheduler": "Task Scheduler (Windows)",
  "in-process": "In-process timer",
  none: "Not installed",
};

const TRIGGER_LABELS: Record<TriggerSource, string> = {
  scheduled: "Scheduled",
  manual: "Manual",
  "self-heal": "Self-heal",
};

const TIMESTAMP_FORMAT = "MMM d, yyyy HH:mm";

/** `[name, min, max]` per cron field, in positional order. */
const CRON_FIELDS: ReadonlyArray<readonly [string, number, number]> = [
  ["minute", 0, 59],
  ["hour", 0, 23],
  ["day of month", 1, 31],
  ["month", 1, 12],
  ["day of week", 0, 6],
];

function validateCronRange(
  range: string,
  min: number,
  max: number,
): string | null {
  const bounds = range.split("-");
  if (bounds.length !== 2) return `'${range}' is not a range`;
  const [lowText, highText] = bounds;
  if (!/^\d+$/.test(lowText) || !/^\d+$/.test(highText)) {
    return `'${range}' is not a numeric range`;
  }
  const low = Number(lowText);
  const high = Number(highText);
  if (low > high || low < min || high > max) {
    return `range ${low}-${high} is outside [${min},${max}]`;
  }
  return null;
}

function validateCronPart(
  part: string,
  min: number,
  max: number,
): string | null {
  const slash = part.indexOf("/");
  if (slash >= 0) {
    const base = part.slice(0, slash);
    const step = part.slice(slash + 1);
    if (!/^\d+$/.test(step) || Number(step) === 0) {
      return `'${step}' is not a positive step`;
    }
    return base === "*" ? null : validateCronRange(base, min, max);
  }
  if (part === "*") return null;
  if (part.includes("-")) return validateCronRange(part, min, max);
  if (!/^\d+$/.test(part)) return `'${part}' is not a number`;
  const value = Number(part);
  return value < min || value > max
    ? `${value} is outside [${min},${max}]`
    : null;
}

function validateCron(expr: string): string | null {
  const fields = expr.trim().split(/\s+/).filter((field) => field.length > 0);
  if (fields.length !== 5) {
    return `Expected 5 fields, got ${fields.length}.`;
  }
  for (let i = 0; i < CRON_FIELDS.length; i += 1) {
    const [name, min, max] = CRON_FIELDS[i];
    for (const part of fields[i].split(",")) {
      const problem = validateCronPart(part, min, max);
      if (problem !== null) return `${name}: ${problem}.`;
    }
  }
  return null;
}

interface StatusRowProps {
  label: string;
  children: ReactNode;
}

function StatusRow({ label, children }: StatusRowProps) {
  return (
    <div className="flex items-center justify-between gap-4 text-sm">
      <span className="text-muted-foreground">{label}</span>
      {children}
    </div>
  );
}

function Timestamp({ iso }: { iso: string | null }) {
  if (iso === null) {
    return <span className="text-muted-foreground">—</span>;
  }
  return (
    <span className="text-right">
      <span className="font-mono text-xs">
        {formatIsoDate(iso, TIMESTAMP_FORMAT)}
      </span>
      <span className="ml-2 text-muted-foreground">{formatRelative(iso)}</span>
    </span>
  );
}

export function SchedulerForm() {
  const { data: config } = useConfig();
  const setConfig = useSetConfig();
  const { data: status } = useSchedulerStatus();
  const setSchedule = useSetSchedulerSchedule();

  // `null` means "no local edit yet", so the field tracks the loaded config
  // until the user actually types — no effect needed to resync.
  const [cronDraft, setCronDraft] = useState<string | null>(null);
  const savedCron = config?.scheduler.cron ?? "";
  const cron = cronDraft ?? savedCron;
  const cronError = validateCron(cron);

  function patchScheduler(patch: Partial<SchedulerConfig>) {
    if (config === undefined) return;
    setConfig.mutate({
      ...config,
      scheduler: { ...config.scheduler, ...patch },
    });
  }

  function commitCron() {
    if (cronError !== null || cron === savedCron) return;
    setSchedule.mutate(cron);
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-2">
        <Label htmlFor="scheduler-cron">Schedule (cron)</Label>
        <Input
          id="scheduler-cron"
          value={cron}
          className="font-mono"
          spellCheck={false}
          autoComplete="off"
          placeholder="0 7-19 * * 1-5"
          aria-invalid={cronError !== null}
          onChange={(event) => setCronDraft(event.target.value)}
          onBlur={commitCron}
          onKeyDown={(event) => {
            if (event.key === "Enter") commitCron();
          }}
        />
        {cronError === null ? (
          <p className="text-xs text-muted-foreground">
            Five POSIX fields: minute hour day-of-month month day-of-week.
          </p>
        ) : (
          <p className="text-xs text-destructive">{cronError}</p>
        )}
      </div>

      <div className="flex flex-col gap-3">
        <div className="flex items-start justify-between gap-6">
          <div className="min-w-0">
            <p className="text-sm font-medium text-foreground">
              Run on a schedule
            </p>
            <p className="text-sm text-muted-foreground">
              Install the system scheduler so standups compile without the app
              open.
            </p>
          </div>
          <Switch
            checked={config?.scheduler.enabled ?? false}
            disabled={config === undefined || setConfig.isPending}
            aria-label="Run on a schedule"
            onCheckedChange={(enabled) => patchScheduler({ enabled })}
          />
        </div>

        <div className="flex items-start justify-between gap-6">
          <div className="min-w-0">
            <p className="text-sm font-medium text-foreground">Self-heal</p>
            <p className="text-sm text-muted-foreground">
              Recompile the previous business day when its AUTO block came out
              empty.
            </p>
          </div>
          <Switch
            checked={config?.scheduler.self_heal ?? false}
            disabled={config === undefined || setConfig.isPending}
            aria-label="Self-heal missed runs"
            onCheckedChange={(selfHeal) =>
              patchScheduler({ self_heal: selfHeal })
            }
          />
        </div>
      </div>

      <div className="flex flex-col gap-3 rounded-lg border border-border bg-inset p-4">
        <StatusRow label="Source">
          <Badge variant={status?.source === "none" ? "secondary" : "default"}>
            {SOURCE_LABELS[status?.source ?? "none"]}
          </Badge>
        </StatusRow>
        <StatusRow label="Next run">
          <Timestamp iso={status?.next_run_at ?? null} />
        </StatusRow>
        <StatusRow label="Last run">
          <Timestamp iso={status?.last_run_at ?? null} />
        </StatusRow>
        <StatusRow label="Last trigger">
          {status?.last_trigger ? (
            <Badge variant="secondary">
              {TRIGGER_LABELS[status.last_trigger]}
            </Badge>
          ) : (
            <span className="text-muted-foreground">—</span>
          )}
        </StatusRow>
      </div>
    </div>
  );
}
