import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import {
  appendPipelineLog,
  clearPipelineLog,
  usePipelineLog,
} from "@/hooks/use-pipeline-log";
import { usePipelineEvents } from "@/hooks/use-pipeline-status";
import type { PipelineLogEvent } from "@/lib/types";
import {
  FIXTURE_DATE,
  FIXTURE_HOST,
  emitTauriEvent,
  resetTauriMocks,
  subscribedEventNames,
} from "@/test/mocks";
import { createTestQueryClient, createWrapper } from "@/test/render";

function makeLine(index: number, detail: string | null = null): PipelineLogEvent {
  return {
    date: FIXTURE_DATE,
    host: FIXTURE_HOST,
    step: "gather",
    level: index % 2 === 0 ? "info" : "done",
    message: `line ${index}`,
    detail,
  };
}

function setup() {
  const queryClient = createTestQueryClient();
  return { queryClient, wrapper: createWrapper(queryClient) };
}

function useSharedLog() {
  usePipelineEvents();
  return usePipelineLog();
}

beforeEach(() => {
  resetTauriMocks();
  clearPipelineLog();
});

describe("appendPipelineLog", () => {
  it("caps the buffer, dropping the oldest lines", () => {
    const lines = [0, 1, 2, 3, 4].reduce(
      (acc, i) => appendPipelineLog(acc, makeLine(i), 3),
      [] as PipelineLogEvent[],
    );
    expect(lines.map((line) => line.message)).toEqual(["line 2", "line 3", "line 4"]);
  });
});

describe("usePipelineLog", () => {
  it("shares the buffer across mounts via the root event subscriber", async () => {
    const { wrapper } = setup();
    const events = renderHook(() => usePipelineEvents(), { wrapper });
    const first = renderHook(() => usePipelineLog(), { wrapper });
    await waitFor(() =>
      expect(subscribedEventNames()).toContain("pipeline-log"),
    );

    act(() => {
      emitTauriEvent("pipeline-log", makeLine(0));
      emitTauriEvent("pipeline-log", makeLine(1, "provider=claude"));
    });

    expect(first.result.current.lines).toHaveLength(2);

    // Opening the panel mounts a second observer — it must see the same lines,
    // not an empty local buffer.
    const second = renderHook(() => usePipelineLog(), { wrapper });
    expect(second.result.current.lines).toHaveLength(2);
    expect(second.result.current.lines[1].detail).toBe("provider=claude");

    second.unmount();
    first.unmount();
    events.unmount();
  });

  it("clears the shared buffer when run-started arrives", async () => {
    const { wrapper } = setup();
    const { result, unmount } = renderHook(() => useSharedLog(), { wrapper });
    await waitFor(() => expect(subscribedEventNames()).toContain("pipeline-log"));

    act(() => {
      emitTauriEvent("pipeline-log", makeLine(0));
    });
    expect(result.current.lines).toHaveLength(1);

    act(() => {
      emitTauriEvent("run-started", {
        run_id: "compile-1",
        kind: "compile",
        title: "Compile standup",
        date: FIXTURE_DATE,
        host: FIXTURE_HOST,
        pipeline: true,
      });
    });
    expect(result.current.lines).toHaveLength(0);
    unmount();
  });

  /// The compile opens a run *and* announces itself, in that order. Clearing on
  /// both events would wipe the run header and every line emitted in between.
  it("keeps the buffer when pipeline-started follows run-started", async () => {
    const { wrapper } = setup();
    const { result, unmount } = renderHook(() => useSharedLog(), { wrapper });
    await waitFor(() => expect(subscribedEventNames()).toContain("pipeline-log"));

    act(() => {
      emitTauriEvent("run-started", {
        run_id: "compile-2",
        kind: "compile",
        title: "Compile standup",
        date: FIXTURE_DATE,
        host: FIXTURE_HOST,
        pipeline: true,
      });
      emitTauriEvent("pipeline-log", makeLine(0));
      emitTauriEvent("pipeline-started", {
        date: FIXTURE_DATE,
        host: FIXTURE_HOST,
        trigger: "manual",
      });
    });
    expect(result.current.lines).toHaveLength(1);
    unmount();
  });

  it("clear() empties the shared buffer", async () => {
    const { wrapper } = setup();
    const { result, unmount } = renderHook(() => useSharedLog(), { wrapper });
    await waitFor(() => expect(subscribedEventNames()).toContain("pipeline-log"));

    act(() => {
      emitTauriEvent("pipeline-log", makeLine(0));
    });
    act(() => {
      result.current.clear();
    });
    expect(result.current.lines).toHaveLength(0);
    unmount();
  });
});
