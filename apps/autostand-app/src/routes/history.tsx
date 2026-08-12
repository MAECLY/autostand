/**
 * History: browse the standup files already on disk.
 *
 * The backend exposes no directory listing — only `read_standup_file(date)` —
 * so the rail probes the last `WINDOW_DAYS` filing dates and treats a
 * rejection as "nothing filed that day". Every probe shares the `["standup",
 * date]` key with the dashboard, so a compile invalidates this list too.
 */

import { useMemo } from "react";

import { useQueries } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { format, subDays } from "date-fns";
import { CalendarX2, FileQuestion } from "lucide-react";

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@autostand/ui/components/alert";
import { Badge } from "@autostand/ui/components/badge";
import { Spinner } from "@autostand/ui/components/spinner";

import { StandupPreview } from "@/components/standup/StandupPreview";
import { useHostSlug } from "@/hooks/use-config";
import { standupKey, useStandupFile } from "@/hooks/use-standup";
import { toAppError } from "@/lib/error";
import { useUiStore } from "@/lib/store";
import { tauriApi } from "@/lib/tauri";
import { cn, formatIsoDate, ISO_DATE_FORMAT } from "@/lib/utils";

export const Route = createFileRoute("/history")({
  component: HistoryPage,
});

/** How far back the rail probes. Each day costs one `read_standup_file` call. */
const WINDOW_DAYS = 14;

function recentDates(today: Date): string[] {
  return Array.from({ length: WINDOW_DAYS }, (_, offset) =>
    format(subDays(today, offset), ISO_DATE_FORMAT),
  );
}

function HistoryPage() {
  // Anchored once per mount: a re-render must not shift the window.
  const dates = useMemo(() => recentDates(new Date()), []);

  const selectedDate = useUiStore((state) => state.selectedDate);
  const setSelectedDate = useUiStore((state) => state.setSelectedDate);
  const hostSlug = useHostSlug();

  const probes = useQueries({
    queries: dates.map((date) => ({
      queryKey: standupKey(date),
      queryFn: () => tauriApi.readStandupFile(date),
    })),
  });

  // Shares the cache entry the probe above filled, and covers a selected date
  // that has since fallen out of the window.
  const selected = useStandupFile(selectedDate);

  const filed = probes.filter((probe) => probe.data !== undefined).length;
  const settled = probes.every((probe) => !probe.isPending);

  return (
    <div className="grid min-h-0 min-w-0 gap-6 p-6 lg:grid-cols-[18rem_minmax(0,1fr)]">
      <aside className="min-h-0 space-y-3 lg:overflow-y-auto">
        <div className="space-y-1">
          <h2 className="text-sm font-semibold text-foreground">
            Last {WINDOW_DAYS} days
          </h2>
          <p className="text-xs text-muted-foreground">
            {settled
              ? `${filed} day${filed === 1 ? "" : "s"} with a standup file`
              : "Checking the dailies directory…"}
          </p>
        </div>

        <ul className="space-y-1">
          {probes.map((probe, index) => {
            const date = dates[index];
            const file = probe.data;
            const isSelected = date === selectedDate;

            return (
              <li key={date}>
                <button
                  type="button"
                  disabled={file === undefined}
                  aria-current={isSelected ? "true" : undefined}
                  onClick={() => setSelectedDate(date)}
                  className={cn(
                    "flex w-full items-center justify-between gap-2 rounded-md px-3 py-2 text-left text-sm transition-colors",
                    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    isSelected
                      ? "bg-muted font-medium text-foreground"
                      : "text-muted-foreground",
                    file !== undefined
                      ? "hover:bg-muted hover:text-foreground"
                      : "opacity-60",
                  )}
                >
                  <span className="truncate">
                    {formatIsoDate(date, "EEE MMM d")}
                  </span>
                  {probe.isPending ? (
                    <Spinner size="sm" label={`Checking ${date}`} />
                  ) : file !== undefined ? (
                    <Badge variant="secondary">
                      {file.auto_blocks.length} host
                      {file.auto_blocks.length === 1 ? "" : "s"}
                    </Badge>
                  ) : (
                    <span className="shrink-0 text-xs text-subtle">no file</span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>

        {settled && filed === 0 && (
          <Alert>
            <CalendarX2 />
            <AlertTitle>Nothing filed yet</AlertTitle>
            <AlertDescription>
              No standup file exists in the dailies directory for these dates.
            </AlertDescription>
          </Alert>
        )}
      </aside>

      <section className="min-w-0">
        {selected.isPending ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner size="sm" label="Loading standup" />
            Loading {formatIsoDate(selectedDate)}…
          </div>
        ) : selected.data !== undefined ? (
          <StandupPreview content={selected.data} hostSlug={hostSlug.data} />
        ) : (
          <Alert>
            <FileQuestion />
            <AlertTitle>No standup file for {formatIsoDate(selectedDate)}</AlertTitle>
            <AlertDescription>
              {selected.error === null
                ? "The dailies directory has no file for this date."
                : toAppError(selected.error).message}
            </AlertDescription>
          </Alert>
        )}
      </section>
    </div>
  );
}
