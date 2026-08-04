import * as React from "react";
import * as ProgressPrimitive from "@radix-ui/react-progress";

import { cn } from "../lib/utils";

export type ProgressProps = React.ComponentProps<typeof ProgressPrimitive.Root> & {
  /** Extra classes for the filled bar (the track is styled via `className`). */
  indicatorClassName?: string;
};

export function Progress({
  className,
  indicatorClassName,
  value,
  max = 100,
  ...props
}: ProgressProps) {
  // Radix keeps `value` unclamped; clamp here so a stale/overshooting value cannot
  // translate the indicator outside the track.
  const percent = max > 0 ? Math.min(100, Math.max(0, ((value ?? 0) / max) * 100)) : 0;

  return (
    <ProgressPrimitive.Root
      value={value}
      max={max}
      className={cn("relative h-2 w-full overflow-hidden rounded-full bg-muted", className)}
      {...props}
    >
      <ProgressPrimitive.Indicator
        className={cn("size-full flex-1 bg-primary transition-transform", indicatorClassName)}
        style={{ transform: `translateX(-${100 - percent}%)` }}
      />
    </ProgressPrimitive.Root>
  );
}
