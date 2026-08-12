/**
 * Terminal-style log body for the bottom panel. Renders the pipeline-log
 * lines with step + level coloring and auto-scrolls while pinned. No header
 * chrome here — the owning `TerminalPanel` provides the VSCode-style header.
 */

import { useEffect, useRef } from "react";

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

  return (
    <div className={cn("flex min-h-0 flex-col bg-inset/50", className)}>
      <div
        ref={scrollRef}
        onScroll={onScroll}
        className="flex-1 overflow-y-auto px-3 py-2"
      >
        {lines.length === 0 ? (
          <p className="text-subtle">No log output yet — run a compile to see pipeline steps.</p>
        ) : (
          <pre className="whitespace-pre-wrap break-words font-mono text-xs leading-relaxed">
            {lines.map((line, i) => (
              <div key={i} className="flex gap-2">
                <span
                  className={cn(
                    "min-w-0 shrink-0",
                    STEP_COLOR[line.step] ?? "text-muted-foreground",
                  )}
                >
                  {line.step}
                </span>
                <span
                  className={cn(
                    "min-w-0 flex-1",
                    LEVEL_CLASS[line.level] ?? "text-foreground",
                  )}
                >
                  {line.message}
                </span>
                {line.detail && (
                  <span className="min-w-0 shrink-0 text-subtle">{line.detail}</span>
                )}
              </div>
            ))}
          </pre>
        )}
      </div>
      <div className="flex shrink-0 items-center justify-end border-t border-border px-2 py-1">
        <button
          type="button"
          onClick={clear}
          disabled={lines.length === 0}
          className="text-xs text-muted-foreground hover:text-foreground disabled:opacity-50"
        >
          Clear
        </button>
      </div>
    </div>
  );
}