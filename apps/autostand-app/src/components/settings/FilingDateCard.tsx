/**
 * Settings → Paths: which standup file a day of work is written into.
 *
 * # Why this lives on Paths
 *
 * Paths is the tab that answers "where does my standup end up": it owns the
 * dailies directory. The filing policy answers the other half of that same
 * question — the *file name* inside it — so the two belong together, and the
 * live preview below reads as a sentence about the directory configured just
 * above.
 *
 * It is deliberately not on Standup Format. That tab molds the LLM prompt and
 * says so in a banner ("presets only affect the LLM render path"); the filing
 * date applies to the deterministic renderer too, so a user who reads that
 * banner would reasonably conclude this setting does nothing in Det mode. Nor
 * is it on Scheduler: that tab is about *when* Autostand runs, which is a
 * different question from which file the run writes.
 *
 * The copy states the consequence rather than the policy name, because the name
 * is what nobody can act on — "next business day" does not tell you that
 * today's work will not appear in today's file.
 */

import { Badge } from "@autostand/ui/components/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";

import { useConfig, useSetConfig } from "@/hooks/use-config";
import { useFilingTarget } from "@/hooks/use-filing-target";
import { windowLabel } from "@/components/standup/StandupTargetHeader";
import type { AppConfig, ArchiveMode } from "@/lib/types";

interface ModeOption {
  id: ArchiveMode;
  label: string;
  /** What changes for the user, in one sentence. Never the internal name. */
  consequence: string;
  detail: string;
  /** Shown on the option that reproduces the original App Script. */
  original: boolean;
}

const MODES: ModeOption[] = [
  {
    id: "next_business_day",
    label: "Next business day",
    consequence: "Today's work is filed for tomorrow's standup.",
    detail:
      "What you did on Thursday is what you report on Friday morning, so Thursday's work appears in Friday's file.",
    original: true,
  },
  {
    id: "same_day",
    label: "Same day",
    consequence: "Today's work is filed for today's standup.",
    detail:
      "What you did on Thursday stays in Thursday's file, and you report it at the end of the day.",
    original: false,
  },
];

/** The default any config without a `dates` block loads as. */
const DEFAULT_ARCHIVE_MODE: ArchiveMode = "next_business_day";

export function FilingDateCard() {
  const config = useConfig();
  const setConfig = useSetConfig();
  const target = useFilingTarget();

  if (config.isPending) {
    return <div className="h-40 animate-pulse rounded-lg bg-muted" />;
  }
  if (config.isError) {
    return (
      <Card>
        <CardContent className="pt-6 text-sm text-muted-foreground">
          Could not load settings.
        </CardContent>
      </Card>
    );
  }

  const current = config.data;
  const selected = current.dates?.archive_mode ?? DEFAULT_ARCHIVE_MODE;

  function select(archive_mode: ArchiveMode) {
    if (archive_mode === selected) return;
    setConfig.mutate(withArchiveMode(current, archive_mode));
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Filing date</CardTitle>
        <CardDescription>
          Which day&apos;s standup file a compile writes your work into.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div
          className="grid gap-3 sm:grid-cols-2"
          role="radiogroup"
          aria-label="Filing date"
        >
          {MODES.map((mode) => {
            const active = selected === mode.id;
            return (
              <button
                key={mode.id}
                type="button"
                role="radio"
                aria-checked={active}
                disabled={setConfig.isPending}
                onClick={() => select(mode.id)}
                className={`flex flex-col items-start gap-1 rounded-lg border p-3 text-left transition-colors ${
                  active
                    ? "border-primary border-2 bg-primary/5"
                    : "border-border hover:border-primary/50"
                }`}
              >
                <div className="flex w-full flex-wrap items-center justify-between gap-2">
                  <span className="text-sm font-medium text-foreground">
                    {mode.label}
                  </span>
                  {mode.original && <Badge variant="outline">Original</Badge>}
                  {active && <Badge variant="default">Active</Badge>}
                </div>
                <span className="text-sm text-foreground">
                  {mode.consequence}
                </span>
                <span className="text-xs text-muted-foreground">
                  {mode.detail}
                </span>
              </button>
            );
          })}
        </div>

        {/* The rule neither option changes, stated once so it is never read as
            a property of whichever option happens to be selected. */}
        <p className="text-xs text-muted-foreground">
          Either way, weekend work accumulates into Monday&apos;s file: no
          standup is ever named after a Saturday or a Sunday, so a Friday
          evening, a Saturday and a Sunday are all reported on Monday.
        </p>

        {target.data !== undefined && (
          <p className="rounded-md border border-border bg-inset p-3 text-sm text-muted-foreground">
            Right now: work done on{" "}
            <span className="font-medium text-foreground">
              {target.data.work_day}
            </span>{" "}
            is filed in{" "}
            <span className="font-mono text-foreground">
              {target.data.filing_date}.md
            </span>
            {target.data.window_empty
              ? "."
              : `, which covers ${windowLabel(
                  target.data.window.range_start,
                  target.data.window.range_end,
                )}.`}
          </p>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * `config` with a different filing policy.
 *
 * Writes the whole `dates` block rather than spreading the old one: a config
 * loaded from a store file written before the block existed has no `dates` key
 * at all, and spreading `undefined` would produce one with no `archive_mode`.
 */
export function withArchiveMode(
  config: AppConfig,
  archive_mode: ArchiveMode,
): AppConfig {
  return { ...config, dates: { archive_mode } };
}
