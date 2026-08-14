/**
 * Visible date windows for History views.
 *
 * The backend lists files with a single `read_dir`; these helpers only decide
 * the inclusive `[since, until]` that query asks for.
 */

import {
  addDays,
  addMonths,
  addWeeks,
  eachDayOfInterval,
  endOfMonth,
  endOfWeek,
  format,
  isValid,
  parseISO,
  startOfMonth,
  startOfWeek,
  subDays,
} from "date-fns";

import type { HistoryView } from "@/lib/store";
import { ISO_DATE_FORMAT } from "@/lib/utils";

const WEEK_OPTS = { weekStartsOn: 1 as const };

export interface DateRange {
  since: string;
  until: string;
}

export function parseFilingDate(iso: string): Date {
  const parsed = parseISO(iso);
  return isValid(parsed) ? parsed : new Date();
}

export function toFilingDate(date: Date): string {
  return format(date, ISO_DATE_FORMAT);
}

export function historyRange(view: HistoryView, anchorIso: string): DateRange {
  const cursor = parseFilingDate(anchorIso);
  switch (view) {
    case "list":
      return {
        since: toFilingDate(subDays(cursor, 13)),
        until: toFilingDate(cursor),
      };
    case "month":
    case "agenda":
      return {
        since: toFilingDate(startOfMonth(cursor)),
        until: toFilingDate(endOfMonth(cursor)),
      };
    case "week":
      return {
        since: toFilingDate(startOfWeek(cursor, WEEK_OPTS)),
        until: toFilingDate(endOfWeek(cursor, WEEK_OPTS)),
      };
    case "day":
      return { since: toFilingDate(cursor), until: toFilingDate(cursor) };
  }
}

export function shiftHistoryAnchor(
  view: HistoryView,
  anchorIso: string,
  direction: -1 | 1,
): string {
  const cursor = parseFilingDate(anchorIso);
  switch (view) {
    case "month":
    case "agenda":
      return toFilingDate(addMonths(cursor, direction));
    case "week":
      return toFilingDate(addWeeks(cursor, direction));
    case "list":
      return toFilingDate(addDays(cursor, direction * 14));
    case "day":
      return toFilingDate(addDays(cursor, direction));
  }
}

export function historyRangeLabel(view: HistoryView, anchorIso: string): string {
  const cursor = parseFilingDate(anchorIso);
  switch (view) {
    case "list":
      return "Last 14 days";
    case "month":
    case "agenda":
      return format(cursor, "MMMM yyyy");
    case "week": {
      const start = startOfWeek(cursor, WEEK_OPTS);
      const end = endOfWeek(cursor, WEEK_OPTS);
      return `${format(start, "MMM d")} – ${format(end, "MMM d, yyyy")}`;
    }
    case "day":
      return format(cursor, "EEEE, MMM d, yyyy");
  }
}

/** Every calendar day in `[since, until]`, inclusive. */
export function daysInRange(since: string, until: string): string[] {
  const start = parseFilingDate(since);
  const end = parseFilingDate(until);
  if (end < start) return [];
  return eachDayOfInterval({ start, end }).map(toFilingDate);
}

export function monthGridDays(anchorIso: string): Date[] {
  const cursor = parseFilingDate(anchorIso);
  const start = startOfWeek(startOfMonth(cursor), WEEK_OPTS);
  const end = endOfWeek(endOfMonth(cursor), WEEK_OPTS);
  return eachDayOfInterval({ start, end });
}

export const WEEKDAY_LABELS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;
