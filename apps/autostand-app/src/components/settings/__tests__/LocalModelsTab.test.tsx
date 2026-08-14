import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { LocalModelsTab } from "@/components/settings/LocalModelsTab";
import { configKey } from "@/hooks/use-config";
import { localModelsKey } from "@/hooks/use-local-models";
import {
  invokeCount,
  makeAppConfig,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import type { LocalModelInfo, LocalRuntimeUnload } from "@/lib/types";
import { createTestQueryClient, renderWithProviders } from "@/test/render";

function makeLocalModel(overrides: Partial<LocalModelInfo> = {}): LocalModelInfo {
  return {
    id: "qwen3.5:2b",
    display_name: "Qwen 3.5 2B (Balanced)",
    tier: "small",
    quality: "balanced",
    format: "GGUF",
    size_bytes: 1_280_835_840,
    context_length: 32_768,
    status: "not_downloaded",
    selected: false,
    license: "Apache-2.0",
    license_url: "https://www.apache.org/licenses/LICENSE-2.0",
    terms_required: false,
    downloaded_bytes: 0,
    runtime_cache_bytes: 0,
    error: null,
    ...overrides,
  };
}

const COLD: LocalRuntimeUnload = {
  processes_terminated: 0,
  caches_removed: 0,
  bytes_freed: 0,
};

function renderTab(models: LocalModelInfo[], unloaded: LocalRuntimeUnload = COLD) {
  mockInvokeCommands({
    get_config: () => makeAppConfig(),
    set_config: () => undefined,
    list_local_models: () => models,
    unload_local_models: () => unloaded,
  });
  const queryClient = createTestQueryClient();
  queryClient.setQueryData(configKey, makeAppConfig());
  queryClient.setQueryData(localModelsKey, models);
  return renderWithProviders(<LocalModelsTab />, { queryClient });
}

function unloadButton(): HTMLElement {
  return screen.getByRole("button", { name: /Unload all models/ });
}

beforeEach(() => {
  resetTauriMocks();
});

describe("LocalModelsTab unload control", () => {
  it("disables the button when no model is installed and nothing is cached", () => {
    renderTab([makeLocalModel()]);

    expect(unloadButton()).toBeDisabled();
  });

  it("enables the button when a model is installed", () => {
    renderTab([makeLocalModel({ status: "available" })]);

    expect(unloadButton()).not.toBeDisabled();
  });

  // A cache can outlive the model file it belongs to, and it is still disk to
  // reclaim.
  it("enables the button for cached runtime state alone", () => {
    renderTab([makeLocalModel({ runtime_cache_bytes: 64 * 1024 ** 2 })]);

    expect(unloadButton()).not.toBeDisabled();
    expect(screen.getByText(/64 MiB/)).toBeInTheDocument();
  });

  it("reports what the backend actually released", async () => {
    renderTab([makeLocalModel({ status: "available", runtime_cache_bytes: 1024 ** 2 })], {
      processes_terminated: 1,
      caches_removed: 2,
      bytes_freed: 3 * 1024 ** 2,
    });

    fireEvent.click(unloadButton());

    await waitFor(() =>
      expect(
        screen.getByText("Freed 1 runtime process stopped and 3 MiB of cached state deleted."),
      ).toBeInTheDocument(),
    );
    expect(invokeCount("unload_local_models")).toBe(1);
  });

  it("says nothing was loaded rather than claiming a phantom unload", async () => {
    renderTab([makeLocalModel({ status: "available" })]);

    fireEvent.click(unloadButton());

    await waitFor(() =>
      expect(
        screen.getByText(
          "Nothing was loaded: no runtime process was running and no cache existed.",
        ),
      ).toBeInTheDocument(),
    );
  });

  it("refetches the catalog so the freed cache size disappears", async () => {
    renderTab([makeLocalModel({ status: "available", runtime_cache_bytes: 1024 ** 2 })], {
      processes_terminated: 0,
      caches_removed: 1,
      bytes_freed: 1024 ** 2,
    });

    fireEvent.click(unloadButton());

    await waitFor(() => expect(invokeCount("list_local_models")).toBeGreaterThan(0));
  });
});
