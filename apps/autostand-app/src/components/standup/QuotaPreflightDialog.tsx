/**
 * The quota pre-flight: what the user needs to know one click before a compile.
 *
 * It states a measured fact and offers the alternative. It never blocks — the
 * primary action is still "compile" and it holds the initial focus, so anyone who
 * already knows their quota dismisses this with Enter and loses nothing. A
 * warning that stops the work would be worse than the quota problem it warns
 * about.
 *
 * Every clause here is conditional on real data. The projection sentence is
 * omitted rather than guessed when the backend declined to project, and the
 * switch offer disappears when no other provider is in a better state.
 */

import { Gauge } from "lucide-react";

import { Button } from "@autostand/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@autostand/ui/components/dialog";

import type { UsagePressure } from "@/lib/usage";

export interface QuotaPreflightDialogProps {
  /** `null` closes the dialog: there is nothing to warn about. */
  pressure: UsagePressure | null;
  /** Provider to offer instead, or `null` when none is in better shape. */
  alternative: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onProceed: () => void;
  onSwitch: (provider: string) => void;
}

/**
 * "Claude — 12% of the 5 h window left, projected to run out in ~35 min."
 *
 * The second clause is dropped, not softened, when there is no projection: an
 * invented countdown is the one thing this contract exists to prevent.
 */
function pressureSentence(pressure: UsagePressure): string {
  const fact = `${pressure.remainingPercent}% of the ${pressure.windowDescription} left`;
  return pressure.runsOutIn === null
    ? `${fact}.`
    : `${fact}, projected to run out in ${pressure.runsOutIn}.`;
}

export function QuotaPreflightDialog({
  pressure,
  alternative,
  open,
  onOpenChange,
  onProceed,
  onSwitch,
}: QuotaPreflightDialogProps) {
  if (pressure === null) return null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Gauge className="size-5" aria-hidden="true" />
            <span className="capitalize">{pressure.provider}</span> is running low
          </DialogTitle>
          <DialogDescription>{pressureSentence(pressure)}</DialogDescription>
        </DialogHeader>

        {alternative !== null ? (
          <p className="text-sm text-muted-foreground">
            <span className="capitalize">{alternative}</span> has more headroom.
            Switching moves it to the front of your provider order — the rest of
            the order is kept.
          </p>
        ) : (
          <p className="text-sm text-muted-foreground">
            No other configured provider is in better shape right now.
          </p>
        )}

        <DialogFooter className="flex-wrap sm:justify-between">
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <div className="flex flex-wrap gap-2">
            {alternative !== null ? (
              <Button
                type="button"
                variant="outline"
                onClick={() => onSwitch(alternative)}
              >
                Use <span className="capitalize">{alternative}</span> instead
              </Button>
            ) : null}
            {/* Autofocused on purpose: the pre-flight informs, it does not gate. */}
            <Button type="button" autoFocus onClick={onProceed}>
              Compile anyway
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
