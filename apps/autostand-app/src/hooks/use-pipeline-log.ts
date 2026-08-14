/**
 * Shared pipeline-log buffer.
 *
 * The viewer, the panel header and the status-bar pill all read this store.
 * Tauri subscription lives in `usePipelineEvents` (mounted once on the root
 * layout) so opening the panel mid-compile does not start an empty buffer.
 */

import { useCallback, useSyncExternalStore } from "react";

import type { PipelineLogEvent } from "@/lib/types";

const DEFAULT_MAX = 500;

let lines: PipelineLogEvent[] = [];
const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of listeners) listener();
}

export function appendPipelineLog(
  current: PipelineLogEvent[],
  line: PipelineLogEvent,
  max = DEFAULT_MAX,
): PipelineLogEvent[] {
  return [...current, line].slice(-max);
}

export function pushPipelineLog(line: PipelineLogEvent, max = DEFAULT_MAX): void {
  lines = appendPipelineLog(lines, line, max);
  emit();
}

export function clearPipelineLog(): void {
  lines = [];
  emit();
}

function subscribe(onStoreChange: () => void): () => void {
  listeners.add(onStoreChange);
  return () => {
    listeners.delete(onStoreChange);
  };
}

function getSnapshot(): PipelineLogEvent[] {
  return lines;
}

export interface UsePipelineLogResult {
  lines: PipelineLogEvent[];
  clear: () => void;
}

export function usePipelineLog(): UsePipelineLogResult {
  const snapshot = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const clear = useCallback(() => {
    clearPipelineLog();
  }, []);
  return { lines: snapshot, clear };
}
