import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(),
}));

import { writeText as tauriWriteText } from "@tauri-apps/plugin-clipboard-manager";

import { useClipboard } from "@/hooks/use-clipboard";

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
  vi.mocked(tauriWriteText).mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
});

describe("useClipboard", () => {
  it("uses the Tauri clipboard plugin when available", async () => {
    const { result } = renderHook(() => useClipboard());

    await act(async () => {
      await result.current.copy("first");
    });

    expect(tauriWriteText).toHaveBeenCalledWith("first");
    expect(navigator.clipboard.writeText).not.toHaveBeenCalled();
    expect(result.current.copied).toBe(true);
  });

  it("falls back to navigator.clipboard when the Tauri plugin throws", async () => {
    vi.mocked(tauriWriteText).mockRejectedValueOnce(new Error("no plugin"));
    const navWrite = navigator.clipboard.writeText as ReturnType<typeof vi.fn>;
    const { result } = renderHook(() => useClipboard());

    await act(async () => {
      await result.current.copy("fallback");
    });

    expect(tauriWriteText).toHaveBeenCalledWith("fallback");
    expect(navWrite).toHaveBeenCalledWith("fallback");
    expect(result.current.copied).toBe(true);
  });

  it("throws when neither clipboard is available", async () => {
    vi.mocked(tauriWriteText).mockRejectedValueOnce(new Error("no plugin"));
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: undefined,
    });
    const { result } = renderHook(() => useClipboard());

    await expect(
      act(async () => {
        await result.current.copy("nothing");
      }),
    ).rejects.toThrow("Clipboard unavailable");
    expect(result.current.copied).toBe(false);
  });

  it("resets the copied flag after resetMs", async () => {
    const { result } = renderHook(() => useClipboard(2000));

    await act(async () => {
      await result.current.copy("temp");
    });
    expect(result.current.copied).toBe(true);

    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(result.current.copied).toBe(false);
  });

  it("clears an in-flight reset timer when a new copy arrives", async () => {
    const { result } = renderHook(() => useClipboard(2000));

    await act(async () => {
      await result.current.copy("a");
    });
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    expect(result.current.copied).toBe(true);

    await act(async () => {
      await result.current.copy("b");
    });
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    expect(result.current.copied).toBe(true);

    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(result.current.copied).toBe(false);
  });

  it("cancels the reset timer on unmount", async () => {
    const { result, unmount } = renderHook(() => useClipboard(2000));

    await act(async () => {
      await result.current.copy("x");
    });
    unmount();

    expect(() => vi.advanceTimersByTime(2000)).not.toThrow();
  });
});