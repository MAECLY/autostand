import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { toast } from "sonner";

import {
  dataSourcesKey,
  useDataSources,
  useToggleDataSource,
} from "@/hooks/use-data-sources";
import type { DataSourceConfig } from "@/lib/types";
import {
  makeDataSource,
  mockInvoke,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import { createTestQueryClient, createWrapper } from "@/test/render";

const SOURCES: DataSourceConfig[] = [
  makeDataSource({ id: "local-git", label: "Local Git", enabled: true }),
  makeDataSource({ id: "github", label: "GitHub", enabled: false }),
];

/** Seed the list without an observer so invalidation cannot refetch it away. */
function setup() {
  const queryClient = createTestQueryClient();
  queryClient.setQueryData(dataSourcesKey, SOURCES);
  return { queryClient, wrapper: createWrapper(queryClient) };
}

function enabledById(sources: DataSourceConfig[] | undefined, id: string) {
  return sources?.find((source) => source.id === id)?.enabled;
}

beforeEach(() => {
  resetTauriMocks();
  vi.mocked(toast.error).mockClear();
});

describe("useDataSources", () => {
  it("lists the sources the backend reports", async () => {
    mockInvokeCommands({ list_data_sources: () => SOURCES });
    const queryClient = createTestQueryClient();

    const { result } = renderHook(() => useDataSources(), {
      wrapper: createWrapper(queryClient),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(SOURCES);
  });
});

describe("useToggleDataSource", () => {
  it("flips the switch before the backend answers", async () => {
    let release = (): void => {};
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    mockInvokeCommands({ toggle_data_source: () => pending });

    const { queryClient, wrapper } = setup();
    const { result } = renderHook(() => useToggleDataSource(), { wrapper });

    act(() => {
      result.current.mutate({ id: "github", enabled: true });
    });

    await waitFor(() =>
      expect(
        enabledById(
          queryClient.getQueryData<DataSourceConfig[]>(dataSourcesKey),
          "github",
        ),
      ).toBe(true),
    );
    // Still in flight — the optimistic value is all the UI has.
    expect(result.current.isPending).toBe(true);

    await act(async () => {
      release();
      await pending;
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(mockInvoke).toHaveBeenCalledWith("toggle_data_source", {
      id: "github",
      enabled: true,
    });
  });

  it("keeps the optimistic value after a successful toggle", async () => {
    mockInvokeCommands({ toggle_data_source: () => undefined });
    const { queryClient, wrapper } = setup();

    const { result } = renderHook(() => useToggleDataSource(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({ id: "github", enabled: true });
    });

    expect(
      enabledById(
        queryClient.getQueryData<DataSourceConfig[]>(dataSourcesKey),
        "github",
      ),
    ).toBe(true);
    expect(vi.mocked(toast.error)).not.toHaveBeenCalled();
  });

  it("rolls back and reports when the backend rejects the toggle", async () => {
    mockInvokeCommands({
      toggle_data_source: () =>
        Promise.reject({
          code: "config",
          message: "local-git is authoritative and cannot be disabled",
        }),
    });
    const { queryClient, wrapper } = setup();

    const { result } = renderHook(() => useToggleDataSource(), { wrapper });

    act(() => {
      result.current.mutate({ id: "local-git", enabled: false });
    });

    await waitFor(() => expect(result.current.isError).toBe(true));

    expect(
      enabledById(
        queryClient.getQueryData<DataSourceConfig[]>(dataSourcesKey),
        "local-git",
      ),
    ).toBe(true);
    expect(vi.mocked(toast.error)).toHaveBeenCalledWith(
      "Toggle data source — config",
      { description: "local-git is authoritative and cannot be disabled" },
    );
  });

  it("leaves the other sources untouched while one toggles", async () => {
    mockInvokeCommands({ toggle_data_source: () => undefined });
    const { queryClient, wrapper } = setup();

    const { result } = renderHook(() => useToggleDataSource(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({ id: "github", enabled: true });
    });

    const sources =
      queryClient.getQueryData<DataSourceConfig[]>(dataSourcesKey);
    expect(sources).toHaveLength(2);
    expect(enabledById(sources, "local-git")).toBe(true);
  });
});
