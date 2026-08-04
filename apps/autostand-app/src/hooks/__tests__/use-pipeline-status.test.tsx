import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { toast } from "sonner";

import { auditSidecarsKey } from "@/hooks/use-audit";
import {
  pipelineStatusKey,
  usePipelineEvents,
  usePipelineStatus,
} from "@/hooks/use-pipeline-status";
import { standupKey } from "@/hooks/use-standup";
import type { PipelineStatus } from "@/lib/types";
import {
  FIXTURE_DATE,
  FIXTURE_HOST,
  emitTauriEvent,
  makeCompileResult,
  makePipelineStatus,
  makeStandupFileContent,
  mockInvokeCommands,
  resetTauriMocks,
  subscribedEventNames,
  tauriListenerCount,
} from "@/test/mocks";
import { createTestQueryClient, createWrapper } from "@/test/render";

const PIPELINE_EVENTS = [
  "pipeline-done",
  "pipeline-error",
  "pipeline-progress",
  "pipeline-started",
  "scheduler-tick",
];

function setup() {
  const queryClient = createTestQueryClient();
  return { queryClient, wrapper: createWrapper(queryClient) };
}

/**
 * Mount the subscriber alone: with no observer on `["pipeline-status"]` an
 * invalidation cannot trigger a background refetch, so the cache reflects
 * exactly what the events wrote.
 */
async function mountEvents() {
  const { queryClient, wrapper } = setup();
  const view = renderHook(() => usePipelineEvents(), { wrapper });

  // `listen` resolves on a microtask; wait for the unsubscribe handles to land.
  await waitFor(() =>
    expect(subscribedEventNames()).toEqual(PIPELINE_EVENTS),
  );

  const status = () =>
    queryClient.getQueryData<PipelineStatus>(pipelineStatusKey);

  return { ...view, queryClient, status };
}

beforeEach(() => {
  resetTauriMocks();
  vi.mocked(toast.success).mockClear();
  vi.mocked(toast.error).mockClear();
  vi.mocked(toast.info).mockClear();
});

describe("usePipelineStatus", () => {
  it("reads the backend status", async () => {
    mockInvokeCommands({
      get_pipeline_status: () =>
        makePipelineStatus({ state: "rendering", percent: 60, step: "render_llm" }),
    });
    const { wrapper } = setup();

    const { result } = renderHook(() => usePipelineStatus(), { wrapper });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toMatchObject({
      state: "rendering",
      percent: 60,
      step: "render_llm",
    });
  });
});

describe("usePipelineEvents", () => {
  it("subscribes to all 5 backend events", async () => {
    const { unmount } = await mountEvents();

    expect(subscribedEventNames()).toEqual(PIPELINE_EVENTS);

    unmount();
  });

  it("unsubscribes every listener on unmount", async () => {
    const { unmount } = await mountEvents();

    unmount();

    await waitFor(() => expect(subscribedEventNames()).toEqual([]));
    for (const name of PIPELINE_EVENTS) {
      expect(tauriListenerCount(name)).toBe(0);
    }
  });

  it("writes a started event into the status cache", async () => {
    const { status, unmount } = await mountEvents();

    act(() => {
      emitTauriEvent("pipeline-started", {
        date: FIXTURE_DATE,
        host: FIXTURE_HOST,
        trigger: "scheduled",
      });
    });

    expect(status()).toMatchObject({
      state: "gathering",
      current_date: FIXTURE_DATE,
      current_host: FIXTURE_HOST,
      percent: 0,
      error: null,
    });

    unmount();
  });

  it("derives the state from the progress step name", async () => {
    const { status, unmount } = await mountEvents();

    act(() => {
      emitTauriEvent("pipeline-progress", {
        date: FIXTURE_DATE,
        host: FIXTURE_HOST,
        step: "gather_github",
        percent: 25,
      });
    });
    expect(status()).toMatchObject({ state: "gathering", percent: 25 });

    act(() => {
      emitTauriEvent("pipeline-progress", {
        date: FIXTURE_DATE,
        host: FIXTURE_HOST,
        step: "render_llm",
        percent: 70,
      });
    });
    expect(status()).toMatchObject({
      state: "rendering",
      step: "render_llm",
      percent: 70,
    });

    unmount();
  });

  it("records the result and invalidates the compiled caches when a run finishes", async () => {
    const { status, queryClient, unmount } = await mountEvents();
    queryClient.setQueryData(standupKey(FIXTURE_DATE), makeStandupFileContent());
    queryClient.setQueryData(auditSidecarsKey(FIXTURE_DATE), []);

    const result = makeCompileResult();
    act(() => {
      emitTauriEvent("pipeline-done", result);
    });

    expect(status()).toMatchObject({
      state: "done",
      percent: 100,
      last_result: result,
      error: null,
    });
    expect(status()?.last_run_at).toEqual(expect.any(String));

    await waitFor(() => {
      expect(
        queryClient.getQueryState(standupKey(FIXTURE_DATE))?.isInvalidated,
      ).toBe(true);
      expect(
        queryClient.getQueryState(auditSidecarsKey(FIXTURE_DATE))?.isInvalidated,
      ).toBe(true);
    });

    unmount();
  });

  it("announces a scheduled run but stays quiet for a manual one", async () => {
    const { unmount } = await mountEvents();

    act(() => {
      emitTauriEvent("pipeline-started", {
        date: FIXTURE_DATE,
        host: FIXTURE_HOST,
        trigger: "manual",
      });
      emitTauriEvent("pipeline-done", makeCompileResult());
    });
    // The compile mutation owns the toast for a run the UI started.
    expect(vi.mocked(toast.success)).not.toHaveBeenCalled();

    act(() => {
      emitTauriEvent("pipeline-started", {
        date: FIXTURE_DATE,
        host: FIXTURE_HOST,
        trigger: "scheduled",
      });
      emitTauriEvent("pipeline-done", makeCompileResult());
    });
    expect(vi.mocked(toast.success)).toHaveBeenCalledWith(
      `Standup compiled — ${FIXTURE_DATE}`,
      { description: "3 bullets across 2 repos" },
    );

    unmount();
  });

  it("moves to the error state and toasts an unattended failure", async () => {
    const { status, unmount } = await mountEvents();

    act(() => {
      emitTauriEvent("pipeline-error", {
        code: "llm",
        message: "claude exited 1",
        step: "render_llm",
        date: FIXTURE_DATE,
      });
    });

    expect(status()).toMatchObject({
      state: "error",
      step: "render_llm",
      current_date: FIXTURE_DATE,
      error: "claude exited 1",
    });
    expect(vi.mocked(toast.error)).toHaveBeenCalledWith("render_llm — llm", {
      description: "claude exited 1",
    });

    unmount();
  });

  it("refreshes the scheduler on a tick", async () => {
    const { queryClient, unmount } = await mountEvents();
    queryClient.setQueryData(["scheduler-status"], { enabled: true });

    act(() => {
      emitTauriEvent("scheduler-tick", {
        next_run_at: "2026-08-04T09:00:00Z",
        source: "launchd",
      });
    });

    await waitFor(() =>
      expect(queryClient.getQueryState(["scheduler-status"])?.isInvalidated).toBe(
        true,
      ),
    );

    unmount();
  });
});
