import { formatIsoDate } from "@/lib/utils";
import { cn } from "@/lib/utils";

export interface AgendaViewProps {
  dates: string[];
  selectedDate: string;
  onSelect: (date: string) => void;
}

export function AgendaView({ dates, selectedDate, onSelect }: AgendaViewProps) {
  if (dates.length === 0) {
    return (
      <p className="px-1 text-xs text-muted-foreground">
        No standup files in this month.
      </p>
    );
  }

  return (
    <ul className="space-y-1">
      {dates.map((date) => {
        const isSelected = date === selectedDate;
        return (
          <li key={date}>
            <button
              type="button"
              aria-current={isSelected ? "true" : undefined}
              onClick={() => onSelect(date)}
              className={cn(
                "flex w-full rounded-md px-3 py-2 text-left text-sm transition-colors",
                "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                isSelected
                  ? "bg-muted font-medium text-foreground"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )}
            >
              {formatIsoDate(date, "EEE MMM d")}
            </button>
          </li>
        );
      })}
    </ul>
  );
}
