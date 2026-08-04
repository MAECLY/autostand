import * as React from "react";

import { cn } from "../lib/utils";

export type CardProps = React.ComponentProps<"div">;

export function Card({ className, ...props }: CardProps) {
  return (
    <div
      className={cn(
        "rounded-lg border border-border bg-card text-card-foreground shadow-sm",
        className,
      )}
      {...props}
    />
  );
}

export type CardHeaderProps = React.ComponentProps<"div">;

export function CardHeader({ className, ...props }: CardHeaderProps) {
  return <div className={cn("flex flex-col gap-1.5 p-6", className)} {...props} />;
}

export type CardTitleProps = React.ComponentProps<"h3">;

export function CardTitle({ className, ...props }: CardTitleProps) {
  return (
    <h3 className={cn("text-lg font-semibold leading-none tracking-tight", className)} {...props} />
  );
}

export type CardDescriptionProps = React.ComponentProps<"p">;

export function CardDescription({ className, ...props }: CardDescriptionProps) {
  return <p className={cn("text-sm text-muted-foreground", className)} {...props} />;
}

export type CardContentProps = React.ComponentProps<"div">;

export function CardContent({ className, ...props }: CardContentProps) {
  return <div className={cn("p-6 pt-0", className)} {...props} />;
}

export type CardFooterProps = React.ComponentProps<"div">;

export function CardFooter({ className, ...props }: CardFooterProps) {
  return (
    <div className={cn("flex items-center justify-end gap-2 p-6 pt-0", className)} {...props} />
  );
}
