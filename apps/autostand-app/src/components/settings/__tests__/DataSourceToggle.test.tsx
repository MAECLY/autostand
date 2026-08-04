import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { DataSourceToggle } from "@/components/settings/DataSourceToggle";
import { makeDataSource, mockInvoke, resetTauriMocks } from "@/test/mocks";
import { renderWithProviders } from "@/test/render";

beforeEach(() => {
  resetTauriMocks();
});

describe("DataSourceToggle", () => {
  it("shows the source label and description", () => {
    renderWithProviders(
      <DataSourceToggle
        source={makeDataSource({ id: "github", label: "GitHub", description: "PRs and reviews." })}
      />,
    );

    expect(screen.getByText("GitHub")).toBeInTheDocument();
    expect(screen.getByText("PRs and reviews.")).toBeInTheDocument();
  });

  it.each(["local-git", "local_git"])(
    "pins %s on and disables its switch",
    (id) => {
      renderWithProviders(
        <DataSourceToggle
          source={makeDataSource({ id, label: "Local Git", enabled: false })}
        />,
      );

      const control = screen.getByRole("switch", { name: "Enable Local Git" });
      expect(control).toBeDisabled();
      // Authoritative source: shown on even though the config says otherwise.
      expect(control).toBeChecked();
    },
  );

  it("ignores a click on the pinned local-git switch", () => {
    renderWithProviders(
      <DataSourceToggle
        source={makeDataSource({ id: "local-git", label: "Local Git", enabled: true })}
      />,
    );

    fireEvent.click(screen.getByRole("switch", { name: "Enable Local Git" }));

    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("enables another source through the toggle command", async () => {
    renderWithProviders(
      <DataSourceToggle
        source={makeDataSource({ id: "github", label: "GitHub", enabled: false })}
      />,
    );

    const control = screen.getByRole("switch", { name: "Enable GitHub" });
    expect(control).toBeEnabled();
    expect(control).not.toBeChecked();

    fireEvent.click(control);

    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("toggle_data_source", {
        id: "github",
        enabled: true,
      }),
    );
  });

  it("disables an enabled source through the toggle command", async () => {
    renderWithProviders(
      <DataSourceToggle
        source={makeDataSource({ id: "opencode", label: "OpenCode", enabled: true })}
      />,
    );

    const control = screen.getByRole("switch", { name: "Enable OpenCode" });
    expect(control).toBeChecked();

    fireEvent.click(control);

    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("toggle_data_source", {
        id: "opencode",
        enabled: false,
      }),
    );
  });
});
