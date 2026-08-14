import { Button } from "@autostand/ui/components/button";

import type { HistoryView } from "@/lib/store";
import { cn } from "@/lib/utils";

const VIEWS: { id: HistoryView; label: string }[] = [
  { id: "list", label: "List" },
  { id: "month", label: "Month" },
  { id: "week", label: "Week" },
  { id: "day", label: "Day" },
  { id: "agenda", label: "Agenda" },
];

export interface HistoryViewToggleProps {
  value: HistoryView;
  onChange: (view: HistoryView) => void;
}

export function HistoryViewToggle({ value, onChange }: HistoryViewToggleProps) {
  return (
    <div
      role="tablist"
      aria-label="History view"
      className="flex flex-wrap gap-1"
    >
      {VIEWS.map((view) => (
        <Button
          key={view.id}
          type="button"
          role="tab"
          aria-selected={value === view.id}
          size="sm"
          variant={value === view.id ? "secondary" : "ghost"}
          className={cn("h-7 px-2 text-xs", value === view.id && "font-medium")}
          onClick={() => onChange(view.id)}
        >
          {view.label}
        </Button>
      ))}
    </div>
  );
}
