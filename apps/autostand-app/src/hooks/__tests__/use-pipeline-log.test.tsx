import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import { usePipelineLog } from "@/hooks/use-pipeline-log";
import type { PipelineLogEvent } from "@/lib/types";
import {
  FIXTURE_DATE,
  FIXTURE_HOST,
  emitTauriEvent,
  resetTauriMocks,
  subscribedEventNames,
  tauriListenerCount,
} from "@/test/mocks";
import { createTestQueryClient, createWrapper } from "@/test/render";

const LOG_EVENTS = ["pipeline-done", "pipeline-log", "pipeline-started"];

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

beforeEach(() => {
  resetTauriMocks();
});

describe("usePipelineLog", () => {
  it("subscribes to pipeline-log, pipeline-started and pipeline-done", async () => {
    const { wrapper } = setup();
    const { unmount } = renderHook(() => usePipelineLog(), { wrapper });

    await waitFor(() => expect(subscribedEventNames()).toEqual(LOG_EVENTS));
    unmount();
  });

  it("accumulates pipeline-log events into the lines buffer", async () => {
    const { wrapper } = setup();
    const { result, unmount } = renderHook(() => usePipelineLog(), { wrapper });
    await waitFor(() => expect(subscribedEventNames()).toEqual(LOG_EVENTS));

    act(() => {
      emitTauriEvent("pipeline-log", makeLine(0));
      emitTauriEvent("pipeline-log", makeLine(1, "provider=claude"));
    });

    expect(result.current.lines).toHaveLength(2);
    expect(result.current.lines[0].message).toBe("line 0");
    expect(result.current.lines[1].detail).toBe("provider=claude");
    unmount();
  });

  it("clears the buffer when pipeline-started arrives", async () => {
    const { wrapper } = setup();
    const { result, unmount } = renderHook(() => usePipelineLog(), { wrapper });
    await waitFor(() => expect(subscribedEventNames()).toEqual(LOG_EVENTS));

    act(() => {
      emitTauriEvent("pipeline-log", makeLine(0));
      emitTauriEvent("pipeline-log", makeLine(1));
    });
    expect(result.current.lines).toHaveLength(2);

    act(() => {
      emitTauriEvent("pipeline-started", {
        date: FIXTURE_DATE,
        host: FIXTURE_HOST,
        trigger: "scheduled",
      });
    });
    expect(result.current.lines).toHaveLength(0);
    unmount();
  });

  it("keeps the buffer after pipeline-done", async () => {
    const { wrapper } = setup();
    const { result, unmount } = renderHook(() => usePipelineLog(), { wrapper });
    await waitFor(() => expect(subscribedEventNames()).toEqual(LOG_EVENTS));

    act(() => {
      emitTauriEvent("pipeline-log", makeLine(0));
    });
    act(() => {
      emitTauriEvent("pipeline-done", {
        date: FIXTURE_DATE,
        host: FIXTURE_HOST,
        status: "ok",
        render_used: "llm",
        fellback: false,
        audit_path: null,
        file_path: "/tmp/x.md",
        accumulated_count: 0,
        message: "ok",
      });
    });
    expect(result.current.lines).toHaveLength(1);
    unmount();
  });

  it("caps the buffer at maxLines, dropping oldest", async () => {
    const { wrapper } = setup();
    const { result, unmount } = renderHook(() => usePipelineLog(3), { wrapper });
    await waitFor(() => expect(subscribedEventNames()).toEqual(LOG_EVENTS));

    act(() => {
      for (let i = 0; i < 5; i += 1) emitTauriEvent("pipeline-log", makeLine(i));
    });

    expect(result.current.lines).toHaveLength(3);
    expect(result.current.lines[0].message).toBe("line 2");
    expect(result.current.lines[2].message).toBe("line 4");
    unmount();
  });

  it("clear() empties the buffer", async () => {
    const { wrapper } = setup();
    const { result, unmount } = renderHook(() => usePipelineLog(), { wrapper });
    await waitFor(() => expect(subscribedEventNames()).toEqual(LOG_EVENTS));

    act(() => {
      emitTauriEvent("pipeline-log", makeLine(0));
    });
    expect(result.current.lines).toHaveLength(1);

    act(() => {
      result.current.clear();
    });
    expect(result.current.lines).toHaveLength(0);
    unmount();
  });

  it("unsubscribes every listener on unmount", async () => {
    const { wrapper } = setup();
    const { unmount } = renderHook(() => usePipelineLog(), { wrapper });
    await waitFor(() => expect(subscribedEventNames()).toEqual(LOG_EVENTS));

    unmount();

    await waitFor(() => expect(subscribedEventNames()).toEqual([]));
    for (const name of LOG_EVENTS) {
      expect(tauriListenerCount(name)).toBe(0);
    }
  });
});