import { CalendarDays } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";

import { formatIsoDate } from "@/lib/utils";

export interface DayViewProps {
  date: string;
  hasFile: boolean;
}

export function DayView({ date, hasFile }: DayViewProps) {
  return (
    <div className="flex items-start gap-3 rounded-md border border-border bg-surface px-3 py-3">
      <CalendarDays className="mt-0.5 size-4 text-muted-foreground" aria-hidden="true" />
      <div className="min-w-0 space-y-1">
        <p className="text-sm font-medium text-foreground">
          {formatIsoDate(date, "EEEE, MMM d")}
        </p>
        {hasFile ? (
          <Badge variant="secondary">Standup on disk</Badge>
        ) : (
          <p className="text-xs text-muted-foreground">No standup file for this date.</p>
        )}
      </div>
    </div>
  );
}
