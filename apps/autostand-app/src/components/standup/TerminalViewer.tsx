/**
 * Terminal-style viewer for pipeline-log lines. Shown on the dashboard below
 * `PipelineCard`. No copy button (per spec). Auto-scrolls to the bottom while
 * the pipeline is running; stops auto-scrolling if the user scrolls up.
 */

import { useEffect, useRef } from "react";
import { Terminal } from "lucide-react";

import { Button } from "@autostand/ui/components/button";

import { usePipelineLog } from "@/hooks/use-pipeline-log";
import { cn } from "@/lib/utils";

const LEVEL_CLASS: Record<string, string> = {
  info: "text-foreground",
  warn: "text-warning",
  error: "text-destructive",
  done: "text-success",
};

const STEP_COLOR: Record<string, string> = {
  window: "text-muted-foreground",
  gather: "text-audit-commit",
  anti_regression: "text-audit-github",
  provenance: "text-audit-review",
  dirty_check: "text-muted-foreground",
  read_existing: "text-muted-foreground",
  render_llm: "text-audit-note",
  validate: "text-audit-note",
  write: "text-audit-commit",
  audit: "text-audit-phantom",
  done: "text-success",
};

export interface TerminalViewerProps {
  className?: string;
}

export function TerminalViewer({ className }: TerminalViewerProps) {
  const { lines, clear } = usePipelineLog();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const pinned = useRef(true);

  useEffect(() => {
    if (pinned.current && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [lines]);

  function onScroll() {
    const el = scrollRef.current;
    if (!el) return;
    const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 4;
    pinned.current = atBottom;
  }

  if (lines.length === 0) {
    return null;
  }

  return (
    <div
      className={cn(
        "rounded-lg border bg-inset/50 font-mono text-xs",
        className,
      )}
    >
      <header className="flex items-center justify-between border-b px-3 py-2">
        <div className="flex items-center gap-2 text-muted-foreground">
          <Terminal className="size-3.5" />
          <span className="font-medium">Pipeline log</span>
          <span className="text-subtle">{lines.length} line(s)</span>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={clear}
          className="h-6 px-2 text-xs"
        >
          Clear
        </Button>
      </header>
      <div
        ref={scrollRef}
        onScroll={onScroll}
        className="max-h-80 overflow-y-auto px-3 py-2"
      >
        <pre className="whitespace-pre-wrap break-words leading-relaxed">
          {lines.map((line, i) => (
            <div key={i} className="flex gap-2">
              <span
                className={cn(
                  "shrink-0",
                  STEP_COLOR[line.step] ?? "text-muted-foreground",
                )}
              >
                {line.step}
              </span>
              <span
                className={cn(
                  "flex-1",
                  LEVEL_CLASS[line.level] ?? "text-foreground",
                )}
              >
                {line.message}
              </span>
              {line.detail && (
                <span className="shrink-0 text-subtle">{line.detail}</span>
              )}
            </div>
          ))}
        </pre>
      </div>
    </div>
  );
}