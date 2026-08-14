/**
 * Dashboard header: the day you worked, and the file that work is filed in.
 *
 * These are two different dates and the app used to print only one of them,
 * labelled "Today". On a Thursday in the default policy the standup being
 * written is Friday's, so a header that said "Today — Aug 13" while the pipeline
 * wrote `2026-08-14.md` left the user unable to find their own standup. Both
 * facts are shown, and the file name is spelled out exactly as it appears on
 * disk so it can be searched for.
 */

import { CalendarDays } from "lucide-react";

import { formatIsoDate } from "@/lib/utils";
import type { FilingTarget } from "@/lib/types";

/** How the gather window reads in the header: one day, or a range. */
export function windowLabel(rangeStart: string, rangeEnd: string): string {
  return rangeStart === rangeEnd
    ? formatIsoDate(rangeEnd)
    : `${formatIsoDate(rangeStart)} – ${formatIsoDate(rangeEnd)}`;
}

/**
 * How the destination file relates to the work day, in plain words.
 *
 * Deliberately not "tomorrow's standup": under the default policy a Friday
 * files into Monday, and naming the wrong weekday is the class of mistake this
 * header exists to end.
 */
export function relationLabel(target: FilingTarget): string {
  return target.filing_date === target.work_day
    ? "today's standup"
    : "the next business day's standup";
}

export interface StandupTargetHeaderProps {
  target: FilingTarget;
}

export function StandupTargetHeader({ target }: StandupTargetHeaderProps) {
  return (
    <div className="min-w-0">
      <h2 className="text-lg font-semibold text-foreground">
        Today&apos;s work — {formatIsoDate(target.work_day)}
      </h2>
      <p className="flex flex-wrap items-center gap-x-1.5 text-sm text-muted-foreground">
        <CalendarDays className="size-4 shrink-0" aria-hidden="true" />
        <span>
          Filed in{" "}
          <span className="font-mono text-foreground">
            {target.filing_date}.md
          </span>{" "}
          — {relationLabel(target)}.
        </span>
        {target.window_empty ? (
          <span>
            Nothing to file yet: {target.filing_date} is ahead of today.
          </span>
        ) : (
          <span>
            Covers {windowLabel(target.window.range_start, target.window.range_end)}.
          </span>
        )}
      </p>
    </div>
  );
}
