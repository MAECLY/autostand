import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "../lib/utils";

/* The `[&>svg~*]:pl-7` selector indents body text around an optional leading icon,
   so callers can drop any lucide icon in as the first child without extra markup. */
export const alertVariants = cva(
  "relative w-full rounded-lg border px-4 py-3 font-sans text-sm [&>svg]:absolute [&>svg]:left-4 [&>svg]:top-4 [&>svg]:size-4 [&>svg~*]:pl-7",
  {
    variants: {
      variant: {
        default: "border-border bg-surface text-foreground [&>svg]:text-info",
        destructive: "border-destructive bg-destructive-bg text-destructive [&>svg]:text-destructive",
        success: "border-success bg-success-bg text-success [&>svg]:text-success",
        warning: "border-warning bg-warning-bg text-warning [&>svg]:text-warning",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

export type AlertProps = React.ComponentProps<"div"> & VariantProps<typeof alertVariants>;

export function Alert({ className, variant, ...props }: AlertProps) {
  return <div role="alert" className={cn(alertVariants({ variant }), className)} {...props} />;
}

export type AlertTitleProps = React.ComponentProps<"h5">;

export function AlertTitle({ className, ...props }: AlertTitleProps) {
  return <h5 className={cn("mb-1 font-medium leading-none tracking-tight", className)} {...props} />;
}

export type AlertDescriptionProps = React.ComponentProps<"div">;

export function AlertDescription({ className, ...props }: AlertDescriptionProps) {
  return <div className={cn("text-sm [&_p]:leading-relaxed", className)} {...props} />;
}
