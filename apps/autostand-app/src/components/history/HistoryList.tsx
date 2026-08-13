import { useQueries } from "@tanstack/react-query";

import { Badge } from "@autostand/ui/components/badge";
import { Spinner } from "@autostand/ui/components/spinner";

import { standupKey } from "@/hooks/use-standup";
import { tauriApi } from "@/lib/tauri";
import { cn, formatIsoDate } from "@/lib/utils";

export interface HistoryListProps {
  dates: string[];
  filed: ReadonlySet<string>;
  selectedDate: string;
  onSelect: (date: string) => void;
}

export function HistoryList({
  dates,
  filed,
  selectedDate,
  onSelect,
}: HistoryListProps) {
  const probes = useQueries({
    queries: dates
      .filter((date) => filed.has(date))
      .map((date) => ({
        queryKey: standupKey(date),
        queryFn: () => tauriApi.readStandupFile(date),
      })),
  });

  const fileByDate = new Map(
    probes.flatMap((probe) =>
      probe.data === undefined ? [] : [[probe.data.date, probe.data] as const],
    ),
  );

  return (
    <ul className="space-y-1">
      {dates.map((date) => {
        const hasFile = filed.has(date);
        const file = fileByDate.get(date);
        const isSelected = date === selectedDate;
        const pending = hasFile && file === undefined;

        return (
          <li key={date}>
            <button
              type="button"
              disabled={!hasFile}
              aria-current={isSelected ? "true" : undefined}
              onClick={() => onSelect(date)}
              className={cn(
                "flex w-full items-center justify-between gap-2 rounded-md px-3 py-2 text-left text-sm transition-colors",
                "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                isSelected
                  ? "bg-muted font-medium text-foreground"
                  : "text-muted-foreground",
                hasFile
                  ? "hover:bg-muted hover:text-foreground"
                  : "opacity-60",
              )}
            >
              <span className="truncate">{formatIsoDate(date, "EEE MMM d")}</span>
              {pending ? (
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
  );
}
