/**
 * The status-bar quota badge.
 *
 * The rule it exists to hold: quota is visible where the user decides to
 * compile, and *only* when a provider actually reported it. A chip that shows
 * "0%" or "—" for a provider nobody measured is worse than no chip at all.
 */

import { screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { StatusBar } from "@/components/layout/StatusBar";
import type { AppConfig, ProviderHealth } from "@/lib/types";
import {
  makeAppConfig,
  makePipelineStatus,
  makeProviderHealth,
  makeUsageWindow,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import { renderWithProviders } from "@/test/render";

const FIVE_HOURS_MS = 5 * 60 * 60 * 1000;

function renderStatusBar(health: ProviderHealth[], config: AppConfig = makeAppConfig()) {
  mockInvokeCommands({
    get_pipeline_status: () => makePipelineStatus(),
    get_host_slug: () => "mbp-miguel",
    get_scheduler_status: () => ({
      enabled: false,
      backend: "in_process",
      cron: "0 9 * * 1-5",
      next_run_at: null,
      last_run_at: null,
      installed: false,
      message: null,
    }),
    get_config: () => config,
    get_provider_health: () => health,
  });
  return renderWithProviders(<StatusBar />);
}

beforeEach(() => {
  resetTauriMocks();
});

describe("StatusBar quota badge", () => {
  it("shows the active provider's tightest window with its projection", async () => {
    renderStatusBar([
      makeProviderHealth({
        provider: "claude",
        windows: [
          makeUsageWindow({ id: "weekly", remaining_percent: 60 }),
          makeUsageWindow({
            id: "five_hour",
            remaining_percent: 12,
            period_duration_ms: FIVE_HOURS_MS,
            runs_out_in_seconds: 2_100,
          }),
        ],
      }),
    ]);

    const detail = await screen.findByText(
      "claude — 12% of the 5 h window left, projected to run out in ~35 min",
    );
    // The chip itself is two glyphs wide; the sentence is the hidden label.
    expect(detail.parentElement).toHaveTextContent("claude12%");
  });

  it("drops the projection clause when the backend declined to project", async () => {
    renderStatusBar([
      makeProviderHealth({
        windows: [
          makeUsageWindow({
            id: "five_hour",
            remaining_percent: 12,
            period_duration_ms: FIVE_HOURS_MS,
          }),
        ],
      }),
    ]);

    expect(
      await screen.findByText("claude — 12% of the 5 h window left"),
    ).toBeInTheDocument();
  });

  /** Absence beats a placeholder: nothing is claimed about an unmeasured provider. */
  it("renders no badge when the provider reported no usable window", async () => {
    renderStatusBar([makeProviderHealth({ windows: [makeUsageWindow()] })]);

    // The bar itself paints, so a missing badge is a decision and not a stall.
    expect(await screen.findByText("mbp-miguel")).toBeInTheDocument();
    expect(screen.queryByText(/% of the/)).not.toBeInTheDocument();
  });

  it("renders no badge when no provider has ever been refreshed", async () => {
    renderStatusBar([]);

    expect(await screen.findByText("mbp-miguel")).toBeInTheDocument();
    expect(screen.queryByText(/% of the/)).not.toBeInTheDocument();
  });

  /**
   * The badge follows `render::provider_chain`: an explicit order wins, so the
   * chip can never name one provider while the render uses another.
   */
  it("tracks the head of the provider order, not the legacy preferred field", async () => {
    const config = makeAppConfig();
    config.llm.preferred_provider = "claude";
    config.llm.provider_order = ["openai", "claude"];

    renderStatusBar(
      [
        makeProviderHealth({
          provider: "claude",
          windows: [makeUsageWindow({ remaining_percent: 3 })],
        }),
        makeProviderHealth({
          provider: "openai",
          windows: [makeUsageWindow({ id: "weekly", remaining_percent: 44 })],
        }),
      ],
      config,
    );

    await waitFor(() => {
      expect(screen.getByText(/^openai — 44%/)).toBeInTheDocument();
    });
    expect(screen.queryByText(/^claude —/)).not.toBeInTheDocument();
  });
});
