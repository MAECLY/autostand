import { describe, expect, it, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(),
}));

import { writeText as tauriWriteText } from "@tauri-apps/plugin-clipboard-manager";

import { renderWithProviders } from "@/test/render";
import { CopyButton } from "@/components/common/CopyButton";
import { screen, fireEvent, waitFor } from "@testing-library/react";

beforeEach(() => {
  vi.clearAllMocks();
  vi.useRealTimers();
  vi.mocked(tauriWriteText).mockResolvedValue(undefined);
});

describe("CopyButton", () => {
  it("renders an icon-only button with the accessible label", () => {
    renderWithProviders(<CopyButton text="hello" label="Copy all" />);
    expect(screen.getByRole("button", { name: "Copy all" })).toBeInTheDocument();
  });

  it("writes the text to the clipboard on click", async () => {
    renderWithProviders(<CopyButton text="payload" label="Copy" />);
    const button = screen.getByRole("button", { name: "Copy" });

    await fireEvent.click(button);

    await waitFor(() =>
      expect(tauriWriteText).toHaveBeenCalledWith("payload"),
    );
  });

  it("flips to the Copied state after a successful click", async () => {
    renderWithProviders(<CopyButton text="x" label="Copy" />);
    const button = screen.getByRole("button", { name: "Copy" });

    await fireEvent.click(button);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Copy" })).toBeInTheDocument(),
    );
  });

  it("shows the tooltip text on hover by default", () => {
    renderWithProviders(<CopyButton text="x" label="Copy section" />);
    expect(
      screen.getByRole("button", { name: "Copy section" }),
    ).toBeInTheDocument();
  });
});