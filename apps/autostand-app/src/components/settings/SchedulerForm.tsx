/** Settings → Scheduler: human-first schedule builder with cron as an escape hatch. */

import { useState, type ReactNode } from "react";
import { CalendarClock, ChevronDown, Clock3 } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@autostand/ui/components/collapsible";
import { Input } from "@autostand/ui/components/input";
import { Label } from "@autostand/ui/components/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@autostand/ui/components/select";
import { Switch } from "@autostand/ui/components/switch";

import { useConfig, useSetConfig } from "@/hooks/use-config";
import {
  useSchedulerStatus,
  useSetSchedulerEnabled,
  useSetSchedulerSchedule,
} from "@/hooks/use-scheduler";
import type { SchedulerConfig, SchedulerSource, TriggerSource } from "@/lib/types";
import { cn, formatIsoDate, formatRelative } from "@/lib/utils";

const SOURCE_LABELS: Record<SchedulerSource, string> = {
  launchd: "macOS background job",
  systemd: "Linux background job",
  "task-scheduler": "Windows background job",
  "in-process": "While Autostand is open",
  none: "Not running",
};

const TRIGGER_LABELS: Record<TriggerSource, string> = {
  scheduled: "Scheduled",
  manual: "Manual",
  "self-heal": "Recovered missed run",
};

const DAYS = [
  { value: 1, short: "M", label: "Monday" },
  { value: 2, short: "T", label: "Tuesday" },
  { value: 3, short: "W", label: "Wednesday" },
  { value: 4, short: "T", label: "Thursday" },
  { value: 5, short: "F", label: "Friday" },
  { value: 6, short: "S", label: "Saturday" },
  { value: 0, short: "S", label: "Sunday" },
] as const;

type ScheduleKind = "once" | "hourly";

export interface HumanSchedule {
  kind: ScheduleKind;
  minute: number;
  hour: number;
  endHour: number;
  days: number[];
}

const DEFAULT_SCHEDULE: HumanSchedule = {
  kind: "once",
  minute: 0,
  hour: 9,
  endHour: 17,
  days: [1, 2, 3, 4, 5],
};

function parseDays(field: string): number[] | null {
  if (field === "*" || field === "0-6") return [0, 1, 2, 3, 4, 5, 6];
  if (field === "1-5") return [1, 2, 3, 4, 5];
  const days = field.split(",").map(Number);
  if (days.some((day) => !Number.isInteger(day) || day < 0 || day > 6)) return null;
  return [...new Set(days)].sort((a, b) => a - b);
}

export function parseHumanSchedule(cron: string): HumanSchedule | null {
  const fields = cron.trim().split(/\s+/);
  if (fields.length !== 5 || fields[2] !== "*" || fields[3] !== "*") return null;
  const minute = Number(fields[0]);
  if (!Number.isInteger(minute) || minute < 0 || minute > 59) return null;
  const days = parseDays(fields[4]);
  if (days === null || days.length === 0) return null;
  const range = /^(\d{1,2})-(\d{1,2})$/.exec(fields[1]);
  if (range !== null) {
    const hour = Number(range[1]);
    const endHour = Number(range[2]);
    if (hour < 0 || endHour > 23 || hour >= endHour) return null;
    return { kind: "hourly", minute, hour, endHour, days };
  }
  const hour = Number(fields[1]);
  if (!Number.isInteger(hour) || hour < 0 || hour > 23) return null;
  return { kind: "once", minute, hour, endHour: Math.min(23, hour + 8), days };
}

function dayField(days: number[]): string {
  const sorted = [...new Set(days)].sort((a, b) => a - b);
  if (sorted.length === 7) return "*";
  if (sorted.join(",") === "1,2,3,4,5") return "1-5";
  return sorted.join(",");
}

export function cronFromHumanSchedule(schedule: HumanSchedule): string {
  const hours =
    schedule.kind === "hourly" ? `${schedule.hour}-${schedule.endHour}` : String(schedule.hour);
  return `${schedule.minute} ${hours} * * ${dayField(schedule.days)}`;
}

function timeLabel(hour: number, minute: number): string {
  return new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(
    new Date(2026, 0, 1, hour, minute),
  );
}

function daysLabel(days: number[]): string {
  const sorted = [...days].sort((a, b) => a - b).join(",");
  if (days.length === 7) return "every day";
  if (sorted === "1,2,3,4,5") return "Monday through Friday";
  if (sorted === "0,6") return "weekends";
  return DAYS.filter((day) => days.includes(day.value))
    .map((day) => day.label.slice(0, 3))
    .join(", ");
}

export function scheduleDescription(schedule: HumanSchedule): string {
  const timing =
    schedule.kind === "hourly"
      ? `Every hour from ${timeLabel(schedule.hour, schedule.minute)} to ${timeLabel(schedule.endHour, schedule.minute)}`
      : `Once at ${timeLabel(schedule.hour, schedule.minute)}`;
  return `${timing}, ${daysLabel(schedule.days)}.`;
}

function validateCron(cron: string): string | null {
  const fields = cron.trim().split(/\s+/);
  if (fields.length !== 5) return "A cron schedule needs five fields.";
  if (!fields.every((field) => /^[\d*/,-]+$/.test(field))) {
    return "Only numbers, *, commas, ranges and steps are supported.";
  }
  return null;
}

function StatusRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 text-sm">
      <span className="text-muted-foreground">{label}</span>
      {children}
    </div>
  );
}

function Timestamp({ iso }: { iso: string | null }) {
  if (iso === null) return <span className="text-muted-foreground">—</span>;
  return (
    <span className="text-right">
      <span className="font-mono text-xs tabular-nums">
        {formatIsoDate(iso, "MMM d, yyyy HH:mm")}
      </span>
      <span className="ml-2 text-muted-foreground">{formatRelative(iso)}</span>
    </span>
  );
}

function HourSelect({
  value,
  minute,
  onChange,
  label,
}: {
  value: number;
  minute: number;
  onChange: (hour: number) => void;
  label: string;
}) {
  return (
    <Select value={String(value)} onValueChange={(next) => onChange(Number(next))}>
      <SelectTrigger aria-label={label}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {Array.from({ length: 24 }, (_, hour) => (
          <SelectItem key={hour} value={String(hour)}>
            {timeLabel(hour, minute)}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

export function SchedulerForm() {
  const { data: config } = useConfig();
  const setConfig = useSetConfig();
  const { data: status } = useSchedulerStatus();
  const setSchedule = useSetSchedulerSchedule();
  const setEnabled = useSetSchedulerEnabled();
  const savedCron = config?.scheduler.cron ?? "0 9 * * 1-5";
  const parsed = parseHumanSchedule(savedCron);
  const [draft, setDraft] = useState<HumanSchedule | null>(null);
  const [advancedOpen, setAdvancedOpen] = useState(parsed === null);
  const [cronDraft, setCronDraft] = useState<string | null>(null);
  const schedule = draft ?? parsed ?? DEFAULT_SCHEDULE;
  const generatedCron = cronFromHumanSchedule(schedule);
  const customCron = cronDraft ?? savedCron;
  const cronError = validateCron(customCron);
  const scheduleError = schedule.days.length === 0
    ? "Choose at least one day."
    : schedule.kind === "hourly" && schedule.hour >= schedule.endHour
      ? "The end time must be after the start time."
      : null;

  function updateSchedule(changes: Partial<HumanSchedule>) {
    setDraft({ ...schedule, ...changes });
  }

  function patchScheduler(patch: Partial<SchedulerConfig>) {
    if (config === undefined) return;
    setConfig.mutate({ ...config, scheduler: { ...config.scheduler, ...patch } });
  }

  function toggleDay(day: number) {
    const days = schedule.days.includes(day)
      ? schedule.days.filter((candidate) => candidate !== day)
      : [...schedule.days, day];
    updateSchedule({ days });
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-start justify-between gap-6 rounded-xl bg-muted/45 p-4">
        <div className="flex min-w-0 gap-3">
          <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-background text-primary">
            <CalendarClock className="size-4" aria-hidden="true" />
          </span>
          <div>
            <p className="text-sm font-semibold">Automatic standups</p>
            <p className="max-w-xl text-sm text-muted-foreground">
              Compile in the background even when the Autostand window is closed.
            </p>
          </div>
        </div>
        <Switch
          checked={config?.scheduler.enabled ?? false}
          disabled={config === undefined || setEnabled.isPending}
          aria-label="Enable automatic standups"
          onCheckedChange={(enabled) => setEnabled.mutate(enabled)}
        />
      </div>

      <fieldset className="flex flex-col gap-3" disabled={config === undefined}>
        <legend className="mb-2 text-sm font-medium">How often?</legend>
        <div className="grid gap-2 sm:grid-cols-2">
          {(["once", "hourly"] as const).map((kind) => (
            <button
              key={kind}
              type="button"
              aria-pressed={schedule.kind === kind}
              onClick={() => updateSchedule({ kind })}
              className={cn(
                "rounded-lg border px-4 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                schedule.kind === kind
                  ? "border-primary bg-primary/5"
                  : "border-border hover:bg-muted/60",
              )}
            >
              <span className="block text-sm font-medium">
                {kind === "once" ? "Once per day" : "Every hour in a window"}
              </span>
              <span className="block text-xs text-muted-foreground">
                {kind === "once" ? "Best for a final daily update" : "Keep the standup current during work hours"}
              </span>
            </button>
          ))}
        </div>
      </fieldset>

      <div className="grid gap-5 sm:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)]">
        <div className="flex flex-col gap-2">
          <Label>{schedule.kind === "once" ? "Run at" : "Run between"}</Label>
          {schedule.kind === "once" ? (
            <Input
              type="time"
              aria-label="Run time"
              value={`${String(schedule.hour).padStart(2, "0")}:${String(schedule.minute).padStart(2, "0")}`}
              onChange={(event) => {
                const [hour, minute] = event.target.value.split(":").map(Number);
                updateSchedule({ hour, minute });
              }}
            />
          ) : (
            <div className="grid grid-cols-[1fr_auto_1fr] items-center gap-2">
              <HourSelect value={schedule.hour} minute={schedule.minute} label="Start hour" onChange={(hour) => updateSchedule({ hour })} />
              <span className="text-xs text-muted-foreground">to</span>
              <HourSelect value={schedule.endHour} minute={schedule.minute} label="End hour" onChange={(endHour) => updateSchedule({ endHour })} />
            </div>
          )}
        </div>

        <div className="flex flex-col gap-2">
          <Label>Run on</Label>
          <div className="grid grid-cols-7 gap-1" role="group" aria-label="Days of week">
            {DAYS.map((day) => (
              <button
                key={day.label}
                type="button"
                aria-label={day.label}
                aria-pressed={schedule.days.includes(day.value)}
                onClick={() => toggleDay(day.value)}
                className={cn(
                  "h-10 rounded-md border text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  schedule.days.includes(day.value)
                    ? "border-primary bg-primary text-primary-foreground"
                    : "border-border text-muted-foreground hover:bg-muted",
                )}
              >
                {day.short}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-4 rounded-lg border border-border bg-inset p-4">
        <div className="flex min-w-0 items-start gap-3">
          <Clock3 className="mt-0.5 size-4 shrink-0 text-primary" aria-hidden="true" />
          <div>
            <p className="text-sm font-medium">{scheduleDescription(schedule)}</p>
            <p className="text-xs text-muted-foreground">Uses your computer&apos;s local time zone.</p>
            {scheduleError !== null ? <p className="mt-1 text-xs text-destructive">{scheduleError}</p> : null}
          </div>
        </div>
        <Button
          type="button"
          disabled={scheduleError !== null || generatedCron === savedCron || setSchedule.isPending}
          onClick={() => setSchedule.mutate(generatedCron, { onSuccess: () => setDraft(null) })}
        >
          {setSchedule.isPending ? "Saving…" : "Save schedule"}
        </Button>
      </div>

      <div className="flex items-start justify-between gap-6 border-t border-border pt-5">
        <div>
          <p className="text-sm font-medium">Recover missed work</p>
          <p className="text-sm text-muted-foreground">
            Recompile the previous business day if its automatic section was empty.
          </p>
        </div>
        <Switch
          checked={config?.scheduler.self_heal ?? false}
          disabled={config === undefined || setConfig.isPending}
          aria-label="Recover missed runs"
          onCheckedChange={(self_heal) => patchScheduler({ self_heal })}
        />
      </div>

      <div className="grid gap-3 rounded-lg border border-border p-4 sm:grid-cols-2">
        <StatusRow label="Background service">
          <Badge variant={status?.source === "none" ? "secondary" : "default"}>
            {SOURCE_LABELS[status?.source ?? "none"]}
          </Badge>
        </StatusRow>
        <StatusRow label="Next run"><Timestamp iso={status?.next_run_at ?? null} /></StatusRow>
        <StatusRow label="Last run"><Timestamp iso={status?.last_run_at ?? null} /></StatusRow>
        <StatusRow label="Last started by">
          {status?.last_trigger ? <Badge variant="secondary">{TRIGGER_LABELS[status.last_trigger]}</Badge> : <span>—</span>}
        </StatusRow>
      </div>

      <Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen}>
        <CollapsibleTrigger className="group flex w-full items-center justify-between rounded-md py-2 text-left text-sm text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
          Advanced schedule (cron)
          <ChevronDown className="size-4 transition-transform group-data-[state=open]:rotate-180" aria-hidden="true" />
        </CollapsibleTrigger>
        <CollapsibleContent className="pt-3">
          <div className="flex flex-col gap-2 rounded-lg bg-muted/35 p-4">
            <Label htmlFor="scheduler-cron">Cron expression</Label>
            <div className="flex gap-2">
              <Input
                id="scheduler-cron"
                value={customCron}
                className="font-mono"
                spellCheck={false}
                autoComplete="off"
                aria-invalid={cronError !== null}
                onChange={(event) => setCronDraft(event.target.value)}
              />
              <Button
                type="button"
                variant="outline"
                disabled={cronError !== null || customCron === savedCron || setSchedule.isPending}
                onClick={() => setSchedule.mutate(customCron, { onSuccess: () => setCronDraft(null) })}
              >
                Apply
              </Button>
            </div>
            <p className={cn("text-xs", cronError === null ? "text-muted-foreground" : "text-destructive")}>
              {cronError ?? "For schedules the visual builder cannot express. Changes still use the same safe validator."}
            </p>
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}
