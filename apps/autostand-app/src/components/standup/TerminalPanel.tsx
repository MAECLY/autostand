/**
 * VSCode-style bottom panel wrapping the TerminalViewer. Provides:
 * - Header chrome (title, line count, minimize, close)
 * - Drag-resize via a 4px top edge handle
 * - Push-layout (takes grid space, shrinks main above)
 * - Open / minimized / closed states via `useUiStore.terminalPanel`
 *
 * Mounted once in `__root.tsx` so it is available on every route.
 */

import { useCallback, useEffect, useRef } from "react";
import { ChevronDown, Terminal, X } from "lucide-react";

import { usePipelineLog } from "@/hooks/use-pipeline-log";
import { useUiStore } from "@/lib/store";

import { TerminalViewer } from "@/components/standup/TerminalViewer";

const MIN_HEIGHT = 80;

export function TerminalPanel() {
  const panel = useUiStore((s) => s.terminalPanel);
  const height = useUiStore((s) => s.terminalPanelHeight);
  const setPanel = useUiStore((s) => s.setTerminalPanel);
  const setHeight = useUiStore((s) => s.setTerminalPanelHeight);
  const { lines } = usePipelineLog();

  const dragging = useRef(false);
  const startY = useRef(0);
  const startH = useRef(0);

  const onPointerMove = useCallback(
    (e: PointerEvent) => {
      if (!dragging.current) return;
      const delta = startY.current - e.clientY;
      const maxH = typeof window !== "undefined" ? window.innerHeight / 2 : 600;
      const next = Math.min(Math.max(MIN_HEIGHT, startH.current + delta), maxH);
      setHeight(next);
    },
    [setHeight],
  );

  const onPointerUp = useCallback(() => {
    dragging.current = false;
    if (typeof document !== "undefined") {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    }
  }, []);

  useEffect(() => {
    if (panel !== "open") return;
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
    };
  }, [panel, onPointerMove, onPointerUp]);

  function startDrag(e: React.PointerEvent) {
    e.preventDefault();
    dragging.current = true;
    startY.current = e.clientY;
    startH.current = height;
    if (typeof document !== "undefined") {
      document.body.style.cursor = "row-resize";
      document.body.style.userSelect = "none";
    }
  }

  if (panel === "closed") return null;

  const panelHeight = panel === "minimized" ? "2rem" : `${height}px`;

  return (
    <div
      className="flex flex-col border-t border-border bg-surface"
      style={{ height: panelHeight }}
      role="region"
      aria-label="Pipeline log panel"
    >
      {panel === "open" && (
        <div
          onPointerDown={startDrag}
          className="h-1 shrink-0 cursor-row-resize bg-transparent hover:bg-primary/30"
          aria-hidden="true"
        />
      )}
      <header className="flex h-8 shrink-0 items-center justify-between border-b border-border bg-surface px-3">
        <div className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <Terminal className="size-3.5 shrink-0" aria-hidden="true" />
          <span className="font-medium">Pipeline log</span>
          <span className="shrink-0 text-subtle">{lines.length} line(s)</span>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {panel === "minimized" ? (
            <button
              type="button"
              onClick={() => setPanel("open")}
              className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
              aria-label="Expand panel"
            >
              <ChevronDown className="size-3.5 rotate-180" />
            </button>
          ) : (
            <button
              type="button"
              onClick={() => setPanel("minimized")}
              className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
              aria-label="Minimize panel"
            >
              <ChevronDown className="size-3.5" />
            </button>
          )}
          <button
            type="button"
            onClick={() => setPanel("closed")}
            className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
            aria-label="Close panel"
          >
            <X className="size-3.5" />
          </button>
        </div>
      </header>
      {panel === "open" && (
        <div className="min-h-0 flex-1 overflow-hidden">
          <TerminalViewer className="h-full" />
        </div>
      )}
    </div>
  );
}