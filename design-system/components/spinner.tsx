import * as React from "react";
import { Loader2 } from "lucide-react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "../lib/utils";

export const spinnerVariants = cva("animate-spin text-muted-foreground", {
  variants: {
    size: {
      sm: "size-3",
      default: "size-4",
      lg: "size-6",
    },
  },
  defaultVariants: {
    size: "default",
  },
});

export interface SpinnerProps
  extends React.ComponentProps<"span">,
    VariantProps<typeof spinnerVariants> {
  /** Announced to screen readers; the glyph itself is decorative. */
  label?: string;
}

export function Spinner({ className, size, label = "Loading", ...props }: SpinnerProps) {
  return (
    <span role="status" className={cn("inline-flex items-center", className)} {...props}>
      <Loader2 aria-hidden="true" className={cn(spinnerVariants({ size }))} />
      <span className="sr-only">{label}</span>
    </span>
  );
}
