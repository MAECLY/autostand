import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { toast } from "sonner";

import {
  useConfig,
  useHostSlug,
  useSetConfig,
  useSetHostSlug,
} from "@/hooks/use-config";
import type { AppConfig } from "@/lib/types";
import {
  type InvokeHandler,
  invokeCount,
  makeAppConfig,
  mockInvoke,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import { createTestQueryClient, createWrapper } from "@/test/render";

/** Minimal in-memory backend so invalidation can be observed as fresh data. */
function stubBackend(overrides: Record<string, InvokeHandler> = {}) {
  const state = { config: makeAppConfig(), slug: "mbp-miguel" };

  mockInvokeCommands({
    get_config: () => state.config,
    set_config: (args) => {
      state.config = args.config as AppConfig;
    },
    get_host_slug: () => state.slug,
    set_host_slug: (args) => {
      state.slug = args.slug as string;
      // The backend mirrors the slug into the config, which is why the mutation
      // invalidates both keys.
      state.config = { ...state.config, host_slug_override: state.slug };
    },
    ...overrides,
  });

  return state;
}

function setup() {
  const queryClient = createTestQueryClient();
  return { queryClient, wrapper: createWrapper(queryClient) };
}

beforeEach(() => {
  resetTauriMocks();
  vi.mocked(toast.success).mockClear();
  vi.mocked(toast.error).mockClear();
});

describe("useConfig", () => {
  it("loads the app config", async () => {
    stubBackend();
    const { wrapper } = setup();

    const { result } = renderHook(() => useConfig(), { wrapper });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.dailies_dir).toBe(
      "/Users/tester/Sync/Github_Dailies",
    );
    expect(invokeCount("get_config")).toBe(1);
  });

  it("surfaces a rejected read as an error", async () => {
    mockInvoke.mockRejectedValue({ code: "config", message: "unreadable" });
    const { wrapper } = setup();

    const { result } = renderHook(() => useConfig(), { wrapper });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(result.current.error).toEqual({
      code: "config",
      message: "unreadable",
    });
  });
});

describe("useSetConfig", () => {
  it("writes the config and refetches it", async () => {
    stubBackend();
    const { wrapper } = setup();

    const { result } = renderHook(
      () => ({ config: useConfig(), save: useSetConfig() }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.config.data?.render_mode).toBe("Auto"));

    await act(async () => {
      await result.current.save.mutateAsync(makeAppConfig({ render_mode: "Det" }));
    });

    expect(mockInvoke).toHaveBeenCalledWith("set_config", {
      config: makeAppConfig({ render_mode: "Det" }),
    });
    // The refetch is what proves `["config"]` was invalidated.
    await waitFor(() => expect(result.current.config.data?.render_mode).toBe("Det"));
    expect(invokeCount("get_config")).toBe(2);
  });

  it("confirms the save with a toast", async () => {
    stubBackend();
    const { wrapper } = setup();

    const { result } = renderHook(() => useSetConfig(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync(makeAppConfig());
    });

    expect(vi.mocked(toast.success)).toHaveBeenCalledWith("Settings saved");
  });

  it("reports a rejected write and leaves the cached config alone", async () => {
    stubBackend({
      set_config: () =>
        Promise.reject({ code: "io", message: "read-only volume" }),
    });
    const { wrapper } = setup();

    const { result } = renderHook(
      () => ({ config: useConfig(), save: useSetConfig() }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.config.isSuccess).toBe(true));

    await act(async () => {
      result.current.save.mutate(makeAppConfig({ render_mode: "Llm" }));
    });

    await waitFor(() => expect(result.current.save.isError).toBe(true));
    expect(vi.mocked(toast.error)).toHaveBeenCalledWith(
      "Save settings — io",
      { description: "read-only volume" },
    );
    expect(result.current.config.data?.render_mode).toBe("Auto");
  });
});

describe("useSetHostSlug", () => {
  it("invalidates both the slug and the config", async () => {
    stubBackend();
    const { wrapper } = setup();

    const { result } = renderHook(
      () => ({
        slug: useHostSlug(),
        config: useConfig(),
        setSlug: useSetHostSlug(),
      }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.slug.data).toBe("mbp-miguel"));
    expect(result.current.config.data?.host_slug_override).toBeNull();

    await act(async () => {
      await result.current.setSlug.mutateAsync("linux-lab");
    });

    await waitFor(() => expect(result.current.slug.data).toBe("linux-lab"));
    await waitFor(() =>
      expect(result.current.config.data?.host_slug_override).toBe("linux-lab"),
    );
    expect(vi.mocked(toast.success)).toHaveBeenCalledWith(
      "Host slug set to linux-lab",
    );
  });
});
