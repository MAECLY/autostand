import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import { useStandupDatesInRange } from "@/hooks/use-standup-dates";
import { invokeCount, mockInvokeCommands, resetTauriMocks } from "@/test/mocks";
import { createTestQueryClient, createWrapper } from "@/test/render";

beforeEach(() => {
  resetTauriMocks();
});

describe("useStandupDatesInRange", () => {
  it("asks the backend for the inclusive window", async () => {
    mockInvokeCommands({
      list_standup_dates: () => ["2026-08-03"],
    });
    const wrapper = createWrapper(createTestQueryClient());

    const { result } = renderHook(
      () => useStandupDatesInRange("2026-08-01", "2026-08-31"),
      { wrapper },
    );

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(["2026-08-03"]);
    expect(invokeCount("list_standup_dates")).toBe(1);
  });
});
