/**
 * Bottom chrome: live pipeline state on the left, machine identity and the next
 * scheduled run on the right. Everything here is read-only backend state.
 */

import { Badge } from "@autostand/ui/components/badge";
import { Progress } from "@autostand/ui/components/progress";
import { Separator } from "@autostand/ui/components/separator";

import { useHostSlug } from "@/hooks/use-config";
import { usePipelineStatus } from "@/hooks/use-pipeline-status";
import { useSchedulerStatus } from "@/hooks/use-scheduler";
import type { PipelineState } from "@/lib/types";
import { formatIsoDate, formatRelative } from "@/lib/utils";

type BadgeVariant = "default" | "secondary" | "success" | "warning" | "error";

const STATE_META: Record<
  PipelineState,
  { label: string; variant: BadgeVariant }
> = {
  idle: { label: "Idle", variant: "secondary" },
  gathering: { label: "Gathering", variant: "default" },
  rendering: { label: "Rendering", variant: "default" },
  done: { label: "Done", variant: "success" },
  error: { label: "Error", variant: "error" },
};

const RUNNING_STATES: readonly PipelineState[] = ["gathering", "rendering"];

export function StatusBar() {
  const { data: pipeline } = usePipelineStatus();
  const { data: hostSlug } = useHostSlug();
  const { data: scheduler } = useSchedulerStatus();

  const state: PipelineState = pipeline?.state ?? "idle";
  const meta = STATE_META[state];
  const running = RUNNING_STATES.includes(state);
  const nextRun = scheduler?.next_run_at ?? null;

  return (
    <footer className="flex h-8 shrink-0 items-center gap-3 border-t border-border bg-surface px-3 text-xs text-muted-foreground">
      <Badge variant={meta.variant}>
        {/* `bg-current` inherits the badge's own token colour. */}
        <span className="mr-1.5 inline-block size-1.5 rounded-full bg-current" aria-hidden />
        {meta.label}
      </Badge>

      <span className="min-w-0 truncate" aria-live="polite">
        {state === "error"
          ? (pipeline?.error ?? "Pipeline failed")
          : (pipeline?.step ?? "No run in progress")}
      </span>

      {running && (
        <span className="flex shrink-0 items-center gap-2">
          <Progress value={pipeline?.percent ?? 0} className="h-1 w-24" />
          <span className="font-mono tabular-nums">{pipeline?.percent ?? 0}%</span>
        </span>
      )}

      <span className="ml-auto flex shrink-0 items-center gap-3">
        <span className="font-mono" title="Host slug">
          {hostSlug ?? "—"}
        </span>

        <Separator orientation="vertical" className="h-4" />

        <span title={nextRun ? formatRelative(nextRun) : undefined}>
          {scheduler?.enabled === false
            ? "Scheduler off"
            : nextRun
              ? `Next run ${formatIsoDate(nextRun, "MMM d, HH:mm")}`
              : "No run scheduled"}
        </span>
      </span>
    </footer>
  );
}
