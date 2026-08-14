import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { toast } from "sonner";

import {
  DEPENDENCY_IDS,
  dependencyGroupKey,
  findDependency,
  useDependencies,
  useRunDependencyRemediation,
} from "@/hooks/use-dependencies";
import { localModelsKey } from "@/hooks/use-local-models";
import { repoSyncStatusKey } from "@/hooks/use-sync";
import {
  invokeCount,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import type { Dependency, RemediationOutcome } from "@/lib/types";
import { createTestQueryClient, createWrapper } from "@/test/render";

function makeDependency(overrides: Partial<Dependency> = {}): Dependency {
  return {
    id: DEPENDENCY_IDS.runtime,
    group: "local_ai",
    label: "llama.cpp runtime",
    description: "Runs GGUF models on this device.",
    state: "missing",
    detail: null,
    remediation: null,
    ...overrides,
  };
}

function makeOutcome(overrides: Partial<RemediationOutcome> = {}): RemediationOutcome {
  return {
    dependency_id: DEPENDENCY_IDS.runtime,
    performed: true,
    message: "llama.cpp runtime is installed.",
    dependency: makeDependency({ state: "ok" }),
    ...overrides,
  };
}

beforeEach(() => {
  resetTauriMocks();
  vi.mocked(toast.success).mockClear();
  vi.mocked(toast.info).mockClear();
});

describe("useDependencies", () => {
  it("reports the group the caller asked for", async () => {
    mockInvokeCommands({ get_dependency_status: () => [makeDependency()] });
    const wrapper = createWrapper(createTestQueryClient());

    const { result } = renderHook(() => useDependencies("local_ai"), { wrapper });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([makeDependency()]);
  });

  // Every probe spawns child processes on the Rust side, so two surfaces
  // showing the same group must cost exactly one probe.
  it("shares one probe between everything rendering the same group", async () => {
    mockInvokeCommands({ get_dependency_status: () => [makeDependency()] });
    const wrapper = createWrapper(createTestQueryClient());

    const first = renderHook(() => useDependencies("local_ai"), { wrapper });
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    const second = renderHook(() => useDependencies("local_ai"), { wrapper });
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));

    expect(invokeCount("get_dependency_status")).toBe(1);
  });

  it("keeps each group in its own cache entry", () => {
    expect(dependencyGroupKey("repo_sync")).not.toEqual(
      dependencyGroupKey("local_ai"),
    );
  });
});

describe("findDependency", () => {
  it("tolerates a query that has not resolved yet", () => {
    expect(findDependency(undefined, DEPENDENCY_IDS.runtime)).toBeUndefined();
    expect(
      findDependency([makeDependency()], DEPENDENCY_IDS.model),
    ).toBeUndefined();
    expect(findDependency([makeDependency()], DEPENDENCY_IDS.runtime)?.label).toBe(
      "llama.cpp runtime",
    );
  });
});

describe("useRunDependencyRemediation", () => {
  it("refreshes everything a satisfied requirement unblocks", async () => {
    mockInvokeCommands({ run_dependency_remediation: () => makeOutcome() });
    const queryClient = createTestQueryClient();
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    const wrapper = createWrapper(queryClient);

    const { result } = renderHook(() => useRunDependencyRemediation(), { wrapper });
    result.current.mutate(DEPENDENCY_IDS.runtime);

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    const invalidated = invalidate.mock.calls.map(([options]) => options?.queryKey);
    expect(invalidated).toContainEqual(repoSyncStatusKey);
    expect(invalidated).toContainEqual(localModelsKey);
    expect(toast.success).toHaveBeenCalledWith("llama.cpp runtime is installed.");
  });

  // Claiming "done" for a step the user still has to take would be a lie the
  // very next probe contradicts.
  it("does not claim success for a step Autostand did not take", async () => {
    mockInvokeCommands({
      run_dependency_remediation: () =>
        makeOutcome({
          performed: false,
          message: "Download a model from the list below.",
        }),
    });
    const wrapper = createWrapper(createTestQueryClient());

    const { result } = renderHook(() => useRunDependencyRemediation(), { wrapper });
    result.current.mutate(DEPENDENCY_IDS.model);

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(toast.success).not.toHaveBeenCalled();
    expect(toast.info).toHaveBeenCalledWith("Download a model from the list below.");
  });

  it("surfaces a failed remediation as an error toast", async () => {
    mockInvokeCommands({
      run_dependency_remediation: () => {
        throw { code: "io", message: "the install did not complete" };
      },
    });
    const wrapper = createWrapper(createTestQueryClient());

    const { result } = renderHook(() => useRunDependencyRemediation(), { wrapper });
    result.current.mutate(DEPENDENCY_IDS.runtime);

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(toast.error).toHaveBeenCalledWith(
      "Fix requirement — io",
      expect.objectContaining({ description: "the install did not complete" }),
    );
  });
});
