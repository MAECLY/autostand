import { describe, expect, it, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import { mockInvoke, resetTauriMocks } from "@/test/mocks";
import { renderWithProviders } from "@/test/render";
import { OpenPathButton } from "@/components/common/OpenPathButton";
import { screen, fireEvent, waitFor } from "@testing-library/react";

const DIR = "/home/tester/Documents/Github";

beforeEach(() => {
  resetTauriMocks();
});

describe("OpenPathButton", () => {
  it("renders an icon-only button with the accessible label", () => {
    renderWithProviders(<OpenPathButton path={DIR} label="Open GitHub directory" />);
    expect(
      screen.getByRole("button", { name: "Open GitHub directory" }),
    ).toBeInTheDocument();
  });

  it("invokes open_in_file_manager with the path on click", async () => {
    renderWithProviders(<OpenPathButton path={DIR} label="Open folder" />);

    await fireEvent.click(screen.getByRole("button", { name: "Open folder" }));

    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("open_in_file_manager", {
        path: DIR,
      }),
    );
  });

  it("stays disabled for a blank path so the shell never sees the cwd", () => {
    renderWithProviders(<OpenPathButton path="   " label="Open folder" />);

    expect(screen.getByRole("button", { name: "Open folder" })).toBeDisabled();
  });

  it("honours the caller's disabled verdict for a missing path", async () => {
    renderWithProviders(<OpenPathButton path={DIR} label="Open folder" disabled />);
    const button = screen.getByRole("button", { name: "Open folder" });

    expect(button).toBeDisabled();
    await fireEvent.click(button);
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("surfaces a rejected open as a toast rather than throwing", async () => {
    mockInvoke.mockRejectedValue({ code: "not_found", message: "path does not exist" });
    renderWithProviders(<OpenPathButton path={DIR} label="Open folder" />);

    await fireEvent.click(screen.getByRole("button", { name: "Open folder" }));

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledTimes(1));
    expect(
      screen.getByRole("button", { name: "Open folder" }),
    ).toBeInTheDocument();
  });
});
