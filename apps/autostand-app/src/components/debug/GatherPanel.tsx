/**
 * One gathered source, collapsed by default.
 *
 * The Debug page exists to show exactly what the pipeline saw, so the body is
 * the raw text — no markdown, no re-wrapping — inside a bounded scroll area.
 */

import { ChevronRight } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@autostand/ui/components/collapsible";
import { ScrollArea } from "@autostand/ui/components/scroll-area";

import { cn } from "@/lib/utils";

export interface GatherPanelProps {
  /** Source name as the pipeline labels it, e.g. `FACTS`, `PRREV`. */
  title: string;
  /** Items behind `text` — repos, notes, sessions, or non-empty lines. */
  count: number;
  /** Raw gathered text, verbatim. */
  text: string;
  /** One line on what this source contributes. */
  description?: string;
  defaultOpen?: boolean;
  className?: string;
}

export function GatherPanel({
  title,
  count,
  text,
  description,
  defaultOpen = false,
  className,
}: GatherPanelProps) {
  const isEmpty = text.trim().length === 0;

  return (
    <Collapsible
      defaultOpen={defaultOpen}
      className={cn(
        "overflow-hidden rounded-lg border border-border bg-surface",
        className,
      )}
    >
      <CollapsibleTrigger className="group flex w-full items-center gap-3 px-3 py-2.5 text-left transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
        <ChevronRight
          aria-hidden
          className="size-4 shrink-0 text-muted-foreground transition-transform group-data-[state=open]:rotate-90"
        />
        <span className="min-w-0 flex-1">
          <span className="block font-mono text-sm font-medium text-foreground">
            {title}
          </span>
          {description !== undefined && (
            <span className="block truncate text-xs text-subtle">
              {description}
            </span>
          )}
        </span>
        <Badge variant={count > 0 ? "secondary" : "outline"}>
          {count} {count === 1 ? "item" : "items"}
        </Badge>
      </CollapsibleTrigger>

      <CollapsibleContent>
        <div className="border-t border-border">
          {isEmpty ? (
            <p className="px-3 py-4 text-sm text-muted-foreground">
              Nothing gathered from this source.
            </p>
          ) : (
            <ScrollArea className="max-h-72">
              <pre className="whitespace-pre-wrap break-words px-3 py-3 font-mono text-xs leading-relaxed text-foreground">
                {text}
              </pre>
            </ScrollArea>
          )}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
