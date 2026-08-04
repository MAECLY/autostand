import * as React from "react";

import { cn } from "../lib/utils";

export type TableProps = React.ComponentProps<"table"> & {
  /** Classes for the scroll wrapper that keeps wide tables from breaking layout. */
  wrapperClassName?: string;
};

export function Table({ className, wrapperClassName, ...props }: TableProps) {
  return (
    <div className={cn("relative w-full overflow-auto", wrapperClassName)}>
      <table
        className={cn("w-full caption-bottom font-sans text-sm text-foreground", className)}
        {...props}
      />
    </div>
  );
}

export type TableHeaderProps = React.ComponentProps<"thead">;

export function TableHeader({ className, ...props }: TableHeaderProps) {
  return <thead className={cn("bg-muted [&_tr]:border-b", className)} {...props} />;
}

export type TableBodyProps = React.ComponentProps<"tbody">;

export function TableBody({ className, ...props }: TableBodyProps) {
  return <tbody className={cn("bg-surface [&_tr:last-child]:border-0", className)} {...props} />;
}

export type TableFooterProps = React.ComponentProps<"tfoot">;

export function TableFooter({ className, ...props }: TableFooterProps) {
  return (
    <tfoot
      className={cn("border-t border-border bg-muted font-medium [&>tr]:last:border-b-0", className)}
      {...props}
    />
  );
}

export type TableRowProps = React.ComponentProps<"tr">;

export function TableRow({ className, ...props }: TableRowProps) {
  return (
    <tr
      className={cn(
        "border-b border-border transition-colors hover:bg-muted data-[state=selected]:bg-muted",
        className,
      )}
      {...props}
    />
  );
}

export type TableHeadProps = React.ComponentProps<"th">;

export function TableHead({ className, ...props }: TableHeadProps) {
  return (
    <th
      className={cn(
        "h-10 px-4 text-left align-middle text-sm font-medium text-muted-foreground [&:has([role=checkbox])]:pr-0",
        className,
      )}
      {...props}
    />
  );
}

export type TableCellProps = React.ComponentProps<"td">;

export function TableCell({ className, ...props }: TableCellProps) {
  return <td className={cn("p-4 align-middle [&:has([role=checkbox])]:pr-0", className)} {...props} />;
}

export type TableCaptionProps = React.ComponentProps<"caption">;

export function TableCaption({ className, ...props }: TableCaptionProps) {
  return <caption className={cn("mt-4 text-sm text-muted-foreground", className)} {...props} />;
}
