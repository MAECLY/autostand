import { isSameMonth, isToday } from "date-fns";

import { monthGridDays, parseFilingDate, toFilingDate, WEEKDAY_LABELS } from "@/components/history/range";
import { cn } from "@/lib/utils";

export interface MonthGridProps {
  anchor: string;
  filed: ReadonlySet<string>;
  selectedDate: string;
  onSelect: (date: string) => void;
}

export function MonthGrid({
  anchor,
  filed,
  selectedDate,
  onSelect,
}: MonthGridProps) {
  const cursor = parseFilingDate(anchor);
  const days = monthGridDays(anchor);

  return (
    <div className="space-y-2">
      <div className="grid grid-cols-7 gap-1 text-center text-[0.65rem] font-medium uppercase tracking-wide text-subtle">
        {WEEKDAY_LABELS.map((label) => (
          <div key={label}>{label}</div>
        ))}
      </div>
      <div className="grid grid-cols-7 gap-1">
        {days.map((day) => {
          const iso = toFilingDate(day);
          const inMonth = isSameMonth(day, cursor);
          const hasFile = filed.has(iso);
          const isSelected = iso === selectedDate;

          return (
            <button
              key={iso}
              type="button"
              aria-current={isSelected ? "true" : undefined}
              aria-label={iso}
              onClick={() => onSelect(iso)}
              className={cn(
                "relative flex aspect-square items-center justify-center rounded-md text-xs transition-colors",
                "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                inMonth ? "text-foreground" : "text-subtle",
                isSelected && "bg-muted font-medium",
                !isSelected && hasFile && "hover:bg-muted",
                isToday(day) && !isSelected && "ring-1 ring-border",
              )}
            >
              {day.getDate()}
              {hasFile ? (
                <span
                  aria-hidden="true"
                  className="absolute bottom-1 size-1 rounded-full bg-primary"
                />
              ) : null}
            </button>
          );
        })}
      </div>
    </div>
  );
}
