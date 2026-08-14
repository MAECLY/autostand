/**
 * Filing-date picker used until `@autostand/ui` publishes Calendar/DatePicker.
 *
 * Same contract as the design-system component: local `YYYY-MM-DD`, Dialog
 * calendar, optional typed ISO field. Swap the import to
 * `@autostand/ui/components/date-picker` when the pin moves.
 */

import { useEffect, useState } from "react";
import {
  addMonths,
  eachDayOfInterval,
  endOfMonth,
  endOfWeek,
  format,
  isSameMonth,
  isToday,
  isValid,
  parseISO,
  startOfMonth,
  startOfWeek,
} from "date-fns";
import { CalendarDays, ChevronLeft, ChevronRight } from "lucide-react";

import { Button } from "@autostand/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@autostand/ui/components/dialog";
import { Input } from "@autostand/ui/components/input";
import { Label } from "@autostand/ui/components/label";

import { cn, formatIsoDate, ISO_DATE_FORMAT } from "@/lib/utils";

const WEEK_OPTS = { weekStartsOn: 1 as const };
const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;

export interface DatePickerProps {
  id?: string;
  value: string;
  onChange: (iso: string) => void;
  disabled?: boolean;
  className?: string;
  placeholder?: string;
}

function parseLocal(iso: string): Date | null {
  const parsed = parseISO(iso);
  return isValid(parsed) ? parsed : null;
}

export function DatePicker({
  id,
  value,
  onChange,
  disabled = false,
  className,
  placeholder = "Pick a date",
}: DatePickerProps) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(value);
  const selected = parseLocal(value) ?? new Date();
  const [visible, setVisible] = useState(startOfMonth(selected));

  useEffect(() => {
    setDraft(value);
  }, [value]);

  const days = eachDayOfInterval({
    start: startOfWeek(startOfMonth(visible), WEEK_OPTS),
    end: endOfWeek(endOfMonth(visible), WEEK_OPTS),
  });

  function commit(next: string) {
    onChange(next);
    setOpen(false);
  }

  function commitTyped() {
    const parsed = parseLocal(draft.trim());
    if (parsed) commit(format(parsed, ISO_DATE_FORMAT));
  }

  return (
    <>
      <Button
        id={id}
        type="button"
        variant="outline"
        disabled={disabled}
        className={cn("justify-start font-normal", className)}
        onClick={() => setOpen(true)}
      >
        <CalendarDays />
        {value.length > 0 ? formatIsoDate(value, "EEE, MMM d, yyyy") : placeholder}
      </Button>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Pick a date</DialogTitle>
            <DialogDescription>
              Choose a filing date from the calendar, or type YYYY-MM-DD.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-3">
            <div className="flex items-center justify-between gap-2">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="size-8 px-0"
                aria-label="Previous month"
                onClick={() => setVisible((current) => addMonths(current, -1))}
              >
                <ChevronLeft />
              </Button>
              <p className="text-sm font-medium text-foreground">
                {format(visible, "MMMM yyyy")}
              </p>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="size-8 px-0"
                aria-label="Next month"
                onClick={() => setVisible((current) => addMonths(current, 1))}
              >
                <ChevronRight />
              </Button>
            </div>

            <div className="grid grid-cols-7 gap-1 text-center text-[0.65rem] font-medium uppercase tracking-wide text-subtle">
              {WEEKDAYS.map((label) => (
                <div key={label}>{label}</div>
              ))}
            </div>

            <div className="grid grid-cols-7 gap-1">
              {days.map((day) => {
                const iso = format(day, ISO_DATE_FORMAT);
                const isSelected = iso === value;
                return (
                  <button
                    key={iso}
                    type="button"
                    aria-current={isSelected ? "true" : undefined}
                    aria-label={iso}
                    onClick={() => commit(iso)}
                    className={cn(
                      "flex aspect-square items-center justify-center rounded-md text-xs transition-colors",
                      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                      isSameMonth(day, visible)
                        ? "text-foreground"
                        : "text-subtle",
                      isSelected && "bg-muted font-medium",
                      !isSelected && "hover:bg-muted",
                      isToday(day) && !isSelected && "ring-1 ring-border",
                    )}
                  >
                    {day.getDate()}
                  </button>
                );
              })}
            </div>
          </div>

          <div className="space-y-2">
            <Label htmlFor={id ? `${id}-typed` : undefined}>ISO date</Label>
            <Input
              id={id ? `${id}-typed` : undefined}
              value={draft}
              spellCheck={false}
              autoComplete="off"
              placeholder="YYYY-MM-DD"
              className="font-mono"
              onChange={(event) => setDraft(event.target.value)}
              onBlur={commitTyped}
              onKeyDown={(event) => {
                if (event.key === "Enter") commitTyped();
              }}
            />
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
