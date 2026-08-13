/**
 * History: browse standup files already on disk.
 *
 * `list_standup_dates` does one `read_dir` for the visible window. Individual
 * files still load through `["standup", date]` so a compile refreshes the
 * preview the same way the dashboard does.
 */

import { useMemo } from "react";

import { createFileRoute } from "@tanstack/react-router";
import { CalendarX2, FileQuestion } from "lucide-react";

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@autostand/ui/components/alert";
import { Spinner } from "@autostand/ui/components/spinner";

import { AgendaView } from "@/components/history/AgendaView";
import { DayView } from "@/components/history/DayView";
import { HistoryList } from "@/components/history/HistoryList";
import { HistoryNavigator } from "@/components/history/HistoryNavigator";
import { HistoryViewToggle } from "@/components/history/HistoryViewToggle";
import { MonthGrid } from "@/components/history/MonthGrid";
import { WeekGrid } from "@/components/history/WeekGrid";
import {
  daysInRange,
  historyRange,
  historyRangeLabel,
  shiftHistoryAnchor,
} from "@/components/history/range";
import { StandupPreview } from "@/components/standup/StandupPreview";
import { useHostSlug } from "@/hooks/use-config";
import { useStandupFile } from "@/hooks/use-standup";
import { useStandupDatesInRange } from "@/hooks/use-standup-dates";
import { toAppError } from "@/lib/error";
import { useUiStore } from "@/lib/store";
import { formatIsoDate, todayIso } from "@/lib/utils";

export const Route = createFileRoute("/history")({
  component: HistoryPage,
});

function HistoryPage() {
  const selectedDate = useUiStore((state) => state.selectedDate);
  const setSelectedDate = useUiStore((state) => state.setSelectedDate);
  const historyView = useUiStore((state) => state.historyView);
  const setHistoryView = useUiStore((state) => state.setHistoryView);
  const historyAnchor = useUiStore((state) => state.historyAnchor);
  const setHistoryAnchor = useUiStore((state) => state.setHistoryAnchor);
  const hostSlug = useHostSlug();

  const range = useMemo(
    () => historyRange(historyView, historyAnchor),
    [historyView, historyAnchor],
  );
  const windowDays = useMemo(
    () => daysInRange(range.since, range.until),
    [range.since, range.until],
  );
  const datesQuery = useStandupDatesInRange(range.since, range.until);
  const filed = useMemo(
    () => new Set(datesQuery.data ?? []),
    [datesQuery.data],
  );

  const selected = useStandupFile(selectedDate);

  const filedCount = datesQuery.data?.length ?? 0;
  const settled = !datesQuery.isPending;

  function selectDate(date: string) {
    setSelectedDate(date);
  }

  return (
    <div className="grid min-h-0 min-w-0 gap-6 p-6 lg:grid-cols-[18rem_minmax(0,1fr)]">
      <aside className="min-h-0 space-y-3 overflow-y-auto">
        <HistoryViewToggle
          value={historyView}
          onChange={(view) => {
            setHistoryView(view);
            if (view === "day") setHistoryAnchor(selectedDate);
          }}
        />
        <HistoryNavigator
          label={historyRangeLabel(historyView, historyAnchor)}
          onPrevious={() => {
            const next = shiftHistoryAnchor(historyView, historyAnchor, -1);
            setHistoryAnchor(next);
            if (historyView === "day") setSelectedDate(next);
          }}
          onNext={() => {
            const next = shiftHistoryAnchor(historyView, historyAnchor, 1);
            setHistoryAnchor(next);
            if (historyView === "day") setSelectedDate(next);
          }}
          onToday={() => {
            const today = todayIso();
            setHistoryAnchor(today);
            setSelectedDate(today);
          }}
        />

        <div className="space-y-1">
          <h2 className="text-sm font-semibold text-foreground">
            {historyRangeLabel(historyView, historyAnchor)}
          </h2>
          <p className="text-xs text-muted-foreground">
            {settled
              ? `${filedCount} day${filedCount === 1 ? "" : "s"} with a standup file`
              : "Checking the dailies directory…"}
          </p>
        </div>

        {historyView === "list" ? (
          <HistoryList
            dates={windowDays.slice().reverse()}
            filed={filed}
            selectedDate={selectedDate}
            onSelect={selectDate}
          />
        ) : null}
        {historyView === "month" ? (
          <MonthGrid
            anchor={historyAnchor}
            filed={filed}
            selectedDate={selectedDate}
            onSelect={selectDate}
          />
        ) : null}
        {historyView === "week" ? (
          <WeekGrid
            dates={windowDays}
            filed={filed}
            selectedDate={selectedDate}
            onSelect={selectDate}
          />
        ) : null}
        {historyView === "day" ? (
          <DayView date={selectedDate} hasFile={filed.has(selectedDate)} />
        ) : null}
        {historyView === "agenda" ? (
          <AgendaView
            dates={datesQuery.data ?? []}
            selectedDate={selectedDate}
            onSelect={selectDate}
          />
        ) : null}

        {settled && filedCount === 0 && historyView === "list" ? (
          <Alert>
            <CalendarX2 />
            <AlertTitle>Nothing filed yet</AlertTitle>
            <AlertDescription>
              No standup file exists in the dailies directory for these dates.
            </AlertDescription>
          </Alert>
        ) : null}
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
