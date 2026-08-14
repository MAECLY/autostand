import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import {
  selectRenderProvenance,
  useAuditSidecars,
  useRenderProvenance,
} from "@/hooks/use-audit";
import {
  FIXTURE_DATE,
  FIXTURE_HOST,
  FIXTURE_OTHER_HOST,
  invokeCount,
  makeAuditSidecar,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import { createTestQueryClient, createWrapper } from "@/test/render";

const SIDECARS = [
  makeAuditSidecar({ host: FIXTURE_OTHER_HOST, provider: "ollama" }),
  makeAuditSidecar(),
];

beforeEach(() => {
  resetTauriMocks();
  mockInvokeCommands({ list_audit_sidecars: () => SIDECARS });
});

describe("selectRenderProvenance", () => {
  it("projects the sidecar of the host that asked", () => {
    expect(selectRenderProvenance(SIDECARS, FIXTURE_HOST)).toEqual({
      provider: "claude",
      model: "claude-sonnet-4",
      fellback: false,
      renderUsed: "llm",
    });
  });

  it("is null for a host with no sidecar", () => {
    expect(selectRenderProvenance(SIDECARS, "never-filed")).toBeNull();
  });
});

describe("useRenderProvenance", () => {
  it("reports this host's provider and model", async () => {
    const { result } = renderHook(
      () => useRenderProvenance(FIXTURE_DATE, FIXTURE_HOST),
      { wrapper: createWrapper(createTestQueryClient()) },
    );

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.provider).toBe("claude");
    expect(result.current.data?.model).toBe("claude-sonnet-4");
  });

  it("is null while the host slug is unknown", async () => {
    const { result } = renderHook(
      () => useRenderProvenance(FIXTURE_DATE, undefined),
      { wrapper: createWrapper(createTestQueryClient()) },
    );

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toBeNull();
  });

  it("shares the sidecar listing instead of fetching its own", async () => {
    const wrapper = createWrapper(createTestQueryClient());
    const { result } = renderHook(
      () => ({
        list: useAuditSidecars(FIXTURE_DATE),
        provenance: useRenderProvenance(FIXTURE_DATE, FIXTURE_HOST),
      }),
      { wrapper },
    );

    await waitFor(() =>
      expect(result.current.provenance.isSuccess).toBe(true),
    );
    expect(result.current.list.data).toHaveLength(2);
    expect(invokeCount("list_audit_sidecars")).toBe(1);
  });
});
