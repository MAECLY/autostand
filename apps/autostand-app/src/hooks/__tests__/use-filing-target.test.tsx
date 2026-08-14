/**
 * The dashboard's filing target must follow the configured policy.
 *
 * The hook does not derive the date — the backend does — so the risk it carries
 * is a *stale cache*: saving a new policy in Settings and leaving the dashboard
 * announcing the file the old one produced, while "Compile now" writes the new
 * one. That dependency is invisible to React Query (the policy is read inside
 * the backend), so it is `useSetConfig` that has to invalidate this query, and
 * that is what these tests pin.
 */

import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { useSetConfig } from "@/hooks/use-config";
import { useFilingTarget } from "@/hooks/use-filing-target";
import type { AppConfig, ArchiveMode } from "@/lib/types";
import {
  invokeCount,
  makeAppConfig,
  makeFilingTarget,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import { createTestQueryClient, createWrapper } from "@/test/render";

/** Backend whose answer depends on the stored policy, as the real one does. */
function stubBackend(initial: ArchiveMode = "next_business_day") {
  const state = { mode: initial };

  mockInvokeCommands({
    get_config: () => configFor(state.mode),
    set_config: (args) => {
      state.mode = (args.config as AppConfig).dates.archive_mode;
    },
    get_filing_target: () =>
      makeFilingTarget({
        filing_date: state.mode === "same_day" ? "2026-08-03" : "2026-08-04",
        archive_mode: state.mode,
      }),
  });

  return state;
}

function configFor(archive_mode: ArchiveMode): AppConfig {
  return { ...makeAppConfig(), dates: { archive_mode } };
}

describe("useFilingTarget", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("resolves the target the backend computed", async () => {
    stubBackend();
    const queryClient = createTestQueryClient();

    const { result } = renderHook(() => useFilingTarget("2026-08-03"), {
      wrapper: createWrapper(queryClient),
    });

    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(result.current.data?.filing_date).toBe("2026-08-04");
    expect(result.current.data?.work_day).toBe("2026-08-03");
  });

  it("refetches after Settings saves a new policy", async () => {
    stubBackend();
    const queryClient = createTestQueryClient();
    const wrapper = createWrapper(queryClient);

    const target = renderHook(() => useFilingTarget("2026-08-03"), { wrapper });
    const save = renderHook(() => useSetConfig(), { wrapper });
    await waitFor(() =>
      expect(target.result.current.data?.filing_date).toBe("2026-08-04"),
    );

    save.result.current.mutate(configFor("same_day"));

    await waitFor(() =>
      expect(target.result.current.data?.filing_date).toBe("2026-08-03"),
    );
    expect(invokeCount("get_filing_target")).toBeGreaterThan(1);
  });

  it("does not fetch the target twice on a cold start", async () => {
    // The policy is not part of the query key, so a config arriving after the
    // first paint must not knock the target's cache entry out from under it.
    stubBackend();
    const queryClient = createTestQueryClient();

    const { result } = renderHook(() => useFilingTarget(), {
      wrapper: createWrapper(queryClient),
    });

    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(invokeCount("get_filing_target")).toBe(1);
  });

  it("still resolves when the config could not be read", async () => {
    // The policy lives in the store the backend reads directly, so a failed
    // `get_config` must not leave the dashboard without a target — it would
    // have nothing to compile and nothing to show.
    mockInvokeCommands({
      get_config: () => {
        throw { code: "config", message: "config store is corrupt" };
      },
      get_filing_target: () => makeFilingTarget(),
    });
    const queryClient = createTestQueryClient();

    const { result } = renderHook(() => useFilingTarget("2026-08-03"), {
      wrapper: createWrapper(queryClient),
    });

    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(result.current.data?.filing_date).toBe("2026-08-04");
  });
});
