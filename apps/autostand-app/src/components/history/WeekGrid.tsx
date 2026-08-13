import { format, isToday, parseISO } from "date-fns";

import { cn, formatIsoDate } from "@/lib/utils";

export interface WeekGridProps {
  dates: string[];
  filed: ReadonlySet<string>;
  selectedDate: string;
  onSelect: (date: string) => void;
}

export function WeekGrid({
  dates,
  filed,
  selectedDate,
  onSelect,
}: WeekGridProps) {
  return (
    <div className="grid grid-cols-7 gap-1">
      {dates.map((date) => {
        const parsed = parseISO(date);
        const hasFile = filed.has(date);
        const isSelected = date === selectedDate;

        return (
          <button
            key={date}
            type="button"
            aria-current={isSelected ? "true" : undefined}
            aria-label={date}
            onClick={() => onSelect(date)}
            className={cn(
              "flex min-w-0 flex-col items-center gap-1 rounded-md px-1 py-2 text-center text-xs transition-colors",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              isSelected
                ? "bg-muted font-medium text-foreground"
                : "text-muted-foreground hover:bg-muted hover:text-foreground",
              isToday(parsed) && !isSelected && "ring-1 ring-border",
            )}
          >
            <span className="text-[0.65rem] uppercase text-subtle">
              {format(parsed, "EEE")}
            </span>
            <span>{formatIsoDate(date, "d")}</span>
            {hasFile ? (
              <span className="size-1 rounded-full bg-primary" aria-hidden="true" />
            ) : (
              <span className="size-1" aria-hidden="true" />
            )}
          </button>
        );
      })}
    </div>
  );
}
