/**
 * Subscribes to `pipeline-log` events and accumulates them into a rolling
 * buffer for the TerminalViewer. Lines are kept in arrival order and capped at
 * `maxLines`; older lines are dropped. A `pipeline-started` / `pipeline-done`
 * cycle for the same date clears the buffer so each compile starts clean.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import { onPipelineDone, onPipelineLog, onPipelineStarted } from "@/lib/tauri";
import type { PipelineLogEvent } from "@/lib/types";

export interface UsePipelineLogResult {
  lines: PipelineLogEvent[];
  clear: () => void;
}

export function usePipelineLog(maxLines = 500): UsePipelineLogResult {
  const [lines, setLines] = useState<PipelineLogEvent[]>([]);
  const buffer = useRef<PipelineLogEvent[]>([]);

  const push = useCallback(
    (line: PipelineLogEvent) => {
      buffer.current = [...buffer.current, line].slice(-maxLines);
      setLines(buffer.current);
    },
    [maxLines],
  );

  const clear = useCallback(() => {
    buffer.current = [];
    setLines([]);
  }, []);

  useEffect(() => {
    const unlistenStarted = onPipelineStarted(() => {
      buffer.current = [];
      setLines([]);
    });
    const unlistenLog = onPipelineLog(push);
    const unlistenDone = onPipelineDone(() => {
      // Keep the buffer after done so the user can read the final output.
      // A new `pipeline-started` clears it for the next run.
    });
    return () => {
      void unlistenStarted.then((fn) => fn());
      void unlistenLog.then((fn) => fn());
      void unlistenDone.then((fn) => fn());
    };
  }, [push]);

  return { lines, clear };
}