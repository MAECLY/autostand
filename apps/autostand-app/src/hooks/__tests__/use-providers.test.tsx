import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { toast } from "sonner";

import {
  providerModelsKey,
  useProviderModels,
  useStoreApiKey,
} from "@/hooks/use-providers";
import {
  invokeCount,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import { createTestQueryClient, createWrapper } from "@/test/render";

beforeEach(() => {
  resetTauriMocks();
  vi.mocked(toast.success).mockClear();
});

describe("useProviderModels", () => {
  it("lists the models the backend reports", async () => {
    mockInvokeCommands({
      list_provider_models: () => ["claude-sonnet-4", "claude-opus-4"],
    });
    const { wrapper } = { wrapper: createWrapper(createTestQueryClient()) };

    const { result } = renderHook(() => useProviderModels("claude"), { wrapper });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(["claude-sonnet-4", "claude-opus-4"]);
    expect(invokeCount("list_provider_models")).toBe(1);
  });

  it("does not probe an empty provider id", () => {
    mockInvokeCommands({ list_provider_models: () => [] });
    const { wrapper } = { wrapper: createWrapper(createTestQueryClient()) };

    const { result } = renderHook(() => useProviderModels(""), { wrapper });

    expect(result.current.fetchStatus).toBe("idle");
    expect(invokeCount("list_provider_models")).toBe(0);
  });
});

describe("useStoreApiKey", () => {
  it("invalidates the model catalogue after a key is stored", async () => {
    mockInvokeCommands({
      store_api_key: () => undefined,
      list_provider_models: () => ["claude-sonnet-4"],
    });
    const queryClient = createTestQueryClient();
    queryClient.setQueryData(providerModelsKey("claude"), [] as string[]);
    const wrapper = createWrapper(queryClient);

    const { result } = renderHook(() => useStoreApiKey(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({ provider: "claude", key: "sk-test" });
    });

    await waitFor(() =>
      expect(queryClient.getQueryState(providerModelsKey("claude"))?.isInvalidated).toBe(
        true,
      ),
    );
  });
});
