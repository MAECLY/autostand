/**
 * Bottom chrome: live pipeline state on the left, machine identity and the next
 * scheduled run on the right. Everything here is read-only backend state.
 *
 * Also hosts the clickable "Terminal" pill that toggles the bottom pipeline-log
 * panel (VSCode-style). A "new output" dot appears on the pill when log lines
 * arrive while the panel is closed.
 */

import { useEffect, useRef } from "react";
import { Terminal } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Progress } from "@autostand/ui/components/progress";
import { Separator } from "@autostand/ui/components/separator";

import { useConfig, useHostSlug } from "@/hooks/use-config";
import { usePipelineLog } from "@/hooks/use-pipeline-log";
import { usePipelineStatus } from "@/hooks/use-pipeline-status";
import { useProviderHealth } from "@/hooks/use-providers";
import { useSchedulerStatus } from "@/hooks/use-scheduler";
import { useUiStore } from "@/lib/store";
import type { PipelineState } from "@/lib/types";
import {
  activeProvider,
  formatRunOut,
  healthFor,
  tightestWindow,
  windowDescription,
} from "@/lib/usage";
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

/**
 * The active provider's tightest quota window, in the width of a chip.
 *
 * It lives here because this is the bar the user is looking at when they decide
 * to compile — quota that only exists inside a Settings tab is quota nobody
 * checks. Two rules keep it honest:
 *
 * 1. **Nothing is better than a placeholder.** No snapshot, or no window with a
 *    readable share, renders nothing at all. A chip reading "—" or "0%" would be
 *    a claim about a provider nobody has measured.
 * 2. **One threshold.** "Low" is `notifications.low_usage_threshold_percent`,
 *    the same value that grades the Settings rail and fires the notification.
 */
function UsageBadge() {
  const { data: config } = useConfig();
  const { data: health } = useProviderHealth();

  const provider = activeProvider(config);
  const tightest = tightestWindow(
    provider === null ? undefined : healthFor(health, provider),
  );
  if (provider === null || tightest === null || config === undefined) return null;

  const remaining = Math.round(tightest.remaining);
  const threshold = config.notifications.low_usage_threshold_percent;
  const runsOut = formatRunOut(tightest.window.runs_out_in_seconds);
  const variant: BadgeVariant =
    remaining <= 0 ? "error" : remaining <= threshold ? "warning" : "secondary";

  const detail = [
    `${provider} — ${remaining}% of the ${windowDescription(tightest.window)} left`,
    runsOut === null ? null : `projected to run out in ${runsOut}`,
  ]
    .filter((part) => part !== null)
    .join(", ");

  return (
    <>
      {/* The chip is two glyphs wide; the whole sentence lives in `title` for a
          hover and in a visually hidden span for a screen reader. `aria-label`
          on a bare span is not reliably announced, so it is not used here. */}
      <Badge variant={variant} title={detail}>
        <span aria-hidden="true" className="capitalize">
          {provider}
        </span>
        <span aria-hidden="true" className="ml-1 font-mono tabular-nums">
          {remaining}%
        </span>
        <span className="sr-only">{detail}</span>
      </Badge>

      <Separator orientation="vertical" className="h-4" />
    </>
  );
}

export function StatusBar() {
  const { data: pipeline } = usePipelineStatus();
  const { data: hostSlug } = useHostSlug();
  const { data: scheduler } = useSchedulerStatus();
  const { lines } = usePipelineLog();
  const panel = useUiStore((s) => s.terminalPanel);
  const setPanel = useUiStore((s) => s.setTerminalPanel);

  const state: PipelineState = pipeline?.state ?? "idle";
  const meta = STATE_META[state];
  const running = RUNNING_STATES.includes(state);
  const nextRun = scheduler?.next_run_at ?? null;

  const lastSeen = useRef(0);
  const hasNew = useRef(false);
  useEffect(() => {
    if (lines.length > lastSeen.current && panel === "closed") {
      hasNew.current = true;
    }
    lastSeen.current = lines.length;
    if (panel !== "closed") hasNew.current = false;
  }, [lines.length, panel]);

  function toggleTerminal() {
    hasNew.current = false;
    setPanel(panel === "closed" ? "open" : "closed");
  }

  return (
    <footer className="flex h-8 shrink-0 items-center gap-3 border-t border-border bg-surface px-3 text-xs text-muted-foreground">
      <button
        type="button"
        onClick={toggleTerminal}
        className="flex shrink-0 items-center gap-1.5 rounded px-1.5 py-0.5 hover:bg-muted"
        aria-label="Toggle pipeline log panel"
        aria-pressed={panel !== "closed"}
      >
        <Terminal className="size-3.5" aria-hidden="true" />
        <span>Terminal</span>
        {hasNew.current && panel === "closed" && (
          <span
            className="size-1.5 rounded-full bg-primary"
            aria-hidden="true"
          />
        )}
      </button>

      <Separator orientation="vertical" className="h-4" />

      <Badge variant={meta.variant}>
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
        <UsageBadge />

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
