/**
 * Composer for the MANUAL region. Items are appended by the backend
 * (`add_manual_item`); autostand never rewrites what is already there.
 */

import { useState } from "react";
import { addDays, format, isValid, parseISO } from "date-fns";
import { Plus } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@autostand/ui/components/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";
import { Label } from "@autostand/ui/components/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@autostand/ui/components/select";
import { Spinner } from "@autostand/ui/components/spinner";
import { Textarea } from "@autostand/ui/components/textarea";

import { DatePicker } from "@/components/common/DatePicker";
import { StandupMarkdown } from "@/components/standup/StandupPreview";
import { useAddManualItem } from "@/hooks/use-standup";
import { ISO_DATE_FORMAT, formatIsoDate } from "@/lib/utils";

type Target = "today" | "tomorrow" | "custom";

/** Filing date one day out; falls back to the input when it is not parseable. */
function nextDay(date: string): string {
  const parsed = parseISO(date);
  return isValid(parsed) ? format(addDays(parsed, 1), ISO_DATE_FORMAT) : date;
}

/** Mirror how the backend appends the note: one bullet per non-empty line. */
function toBullets(text: string): string {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => `- ${line.replace(/^-\s*/, "")}`)
    .join("\n");
}

export interface ManualEditorProps {
  /** Filing date of the file being edited, `YYYY-MM-DD`. */
  date: string;
  initialContent?: string;
}

export function ManualEditor({ date, initialContent = "" }: ManualEditorProps) {
  const [text, setText] = useState(initialContent);
  const [target, setTarget] = useState<Target>("today");
  const [customDate, setCustomDate] = useState(date);
  const addManualItem = useAddManualItem();

  const targetDate =
    target === "today" ? date : target === "tomorrow" ? nextDay(date) : customDate;
  const bullets = toBullets(text);
  const pending = addManualItem.isPending;
  const canSubmit = bullets.length > 0 && !pending;

  function handleAdd() {
    if (!canSubmit) return;
    addManualItem.mutate(
      { date: targetDate, item: bullets },
      {
        onSuccess: () => {
          setText("");
          toast.success(`Added to ${formatIsoDate(targetDate)}`);
        },
      },
    );
  }

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between gap-2">
        <CardTitle className="text-sm font-medium text-subtle">
          Add manual item
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="manual-item">Note</Label>
          <Textarea
            id="manual-item"
            className="min-h-24"
            placeholder="Attended architecture review meeting at 14:00"
            value={text}
            disabled={pending}
            onChange={(event) => setText(event.target.value)}
          />
        </div>

        <div className="flex flex-wrap items-end gap-3">
          <div className="space-y-2">
            <Label htmlFor="manual-target">File on</Label>
            <Select
              value={target}
              disabled={pending}
              onValueChange={(value) => setTarget(value as Target)}
            >
              <SelectTrigger id="manual-target" className="w-56">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="today" textValue="Today">
                  Today — {date}
                </SelectItem>
                <SelectItem value="tomorrow" textValue="Tomorrow">
                  Tomorrow — {nextDay(date)}
                </SelectItem>
                <SelectItem value="custom" textValue="Custom">
                  Custom…
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          {target === "custom" ? (
            <DatePicker
              id="manual-custom-date"
              value={customDate}
              disabled={pending}
              className="w-56"
              onChange={setCustomDate}
            />
          ) : null}

          <Button type="button" disabled={!canSubmit} onClick={handleAdd}>
            {pending ? (
              <>
                <Spinner size="sm" label="Adding" />
                Adding…
              </>
            ) : (
              <>
                <Plus />
                Add
              </>
            )}
          </Button>
        </div>

        <div className="space-y-2">
          <p className="text-sm font-medium text-foreground">Preview</p>
          <div className="rounded-md border border-border bg-inset p-3">
            {bullets.length > 0 ? (
              <StandupMarkdown>{bullets}</StandupMarkdown>
            ) : (
              <p className="text-sm text-muted-foreground">
                Nothing to add yet.
              </p>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
