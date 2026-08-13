import { ChevronLeft, ChevronRight } from "lucide-react";

import { Button } from "@autostand/ui/components/button";

export interface HistoryNavigatorProps {
  label: string;
  onPrevious: () => void;
  onNext: () => void;
  onToday: () => void;
}

export function HistoryNavigator({
  label,
  onPrevious,
  onNext,
  onToday,
}: HistoryNavigatorProps) {
  return (
    <div className="flex items-center justify-between gap-2">
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="size-8 px-0"
        aria-label="Previous range"
        onClick={onPrevious}
      >
        <ChevronLeft />
      </Button>
      <div className="min-w-0 flex-1 text-center">
        <p className="truncate text-sm font-medium text-foreground">{label}</p>
        <Button
          type="button"
          variant="link"
          size="sm"
          className="h-auto p-0 text-xs"
          onClick={onToday}
        >
          Today
        </Button>
      </div>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="size-8 px-0"
        aria-label="Next range"
        onClick={onNext}
      >
        <ChevronRight />
      </Button>
    </div>
  );
}
