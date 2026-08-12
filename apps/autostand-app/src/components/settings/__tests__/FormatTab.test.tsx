import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import { screen, within } from "@testing-library/react";
import { fireEvent, waitFor } from "@testing-library/react";

import { FormatTab } from "@/components/settings/FormatTab";
import { configKey } from "@/hooks/use-config";
import { makeAppConfig, mockInvokeCommands, resetTauriMocks } from "@/test/mocks";
import { createTestQueryClient, renderWithProviders } from "@/test/render";

const BASE_CONFIG = makeAppConfig();

function stubConfig(config = BASE_CONFIG) {
  mockInvokeCommands({
    get_config: () => config,
    set_config: () => undefined,
  });
}

function renderFormatTab(config = BASE_CONFIG) {
  const queryClient = createTestQueryClient();
  queryClient.setQueryData(configKey, config);
  return renderWithProviders(<FormatTab />, { queryClient });
}

describe("FormatTab", () => {
  beforeEach(() => {
    resetTauriMocks();
    stubConfig();
  });

  it("renders all 13 preset buttons", () => {
    renderFormatTab();

    const labels = [
      "Classic Scrum 3Q",
      "Scrum 4Q",
      "Mad / Sad / Glad",
      "Start / Stop / Continue",
      "Keep / Drop / Create",
      "5Q + Confidence",
      "Spotify 4Q",
      "Async Status",
      "Walking / 15-min",
      "Walk the Board",
      "Y / T / B / R",
      "Decisions & Commitments",
      "OKR-tied",
    ];
    for (const label of labels) {
      expect(screen.getAllByText(label).length).toBeGreaterThan(0);
    }
  });

  it("shows the Active badge on the saved preset", () => {
    renderFormatTab();

    const presetButtons = screen.getAllByRole("button", { name: /Classic Scrum 3Q/ });
    const card = presetButtons[0];
    expect(card).not.toBeNull();
    expect(within(card).getByText("Active")).toBeDefined();
  });

  it("fires set_config when a preset is clicked", async () => {
    const { queryClient } = renderFormatTab();

    fireEvent.click(screen.getByText("Mad / Sad / Glad"));

    await waitFor(() => {
      expect(queryClient.isFetching({ queryKey: configKey })).toBe(0);
    });
  });

  it("shows the Det warning when render_mode is Det", () => {
    stubConfig({ ...BASE_CONFIG, render_mode: "Det" });
    renderFormatTab({ ...BASE_CONFIG, render_mode: "Det" });

    expect(screen.getByText(/Render mode is/)).toBeDefined();
  });

  it("renders the preview caption", () => {
    renderFormatTab();

    expect(screen.getByText("Example — not a real standup")).toBeDefined();
  });

  it("renders the verbosity Select", () => {
    renderFormatTab();

    expect(screen.getByText("Verbosity")).toBeDefined();
  });

  it("renders the conventional-commit switch", () => {
    renderFormatTab();

    expect(screen.getByText("Conventional-commit scopes")).toBeDefined();
  });

  it("renders the Reset to defaults button", () => {
    renderFormatTab();

    expect(screen.getByText("Reset to defaults")).toBeDefined();
  });
});