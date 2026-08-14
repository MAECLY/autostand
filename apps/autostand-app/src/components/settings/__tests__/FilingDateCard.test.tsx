/**
 * Settings → Paths → Filing date.
 *
 * Two things have to hold: the copy states the *consequence* of each policy
 * (the internal name is what nobody can act on), and choosing one writes a
 * complete `dates` block — including onto a config that has no such block yet,
 * which is what every install upgraded from an earlier build looks like.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import { fireEvent, screen, waitFor, within } from "@testing-library/react";

import {
  FilingDateCard,
  withArchiveMode,
} from "@/components/settings/FilingDateCard";
import { configKey } from "@/hooks/use-config";
import {
  makeAppConfig,
  makeFilingTarget,
  mockInvoke,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import { createTestQueryClient, renderWithProviders } from "@/test/render";
import type { AppConfig } from "@/lib/types";

const BASE_CONFIG = makeAppConfig();

function renderCard(config: AppConfig = BASE_CONFIG) {
  mockInvokeCommands({
    get_config: () => config,
    set_config: () => undefined,
    get_filing_target: () =>
      makeFilingTarget({
        filing_date:
          config.dates.archive_mode === "same_day" ? "2026-08-03" : "2026-08-04",
        archive_mode: config.dates.archive_mode,
      }),
  });
  const queryClient = createTestQueryClient();
  queryClient.setQueryData(configKey, config);
  return renderWithProviders(<FilingDateCard />, { queryClient });
}

/** The `set_config` payload of the most recent call, or `null`. */
function lastSavedConfig(): AppConfig | null {
  const call = mockInvoke.mock.calls
    .filter(([command]) => command === "set_config")
    .at(-1);
  return call === undefined ? null : (call[1] as { config: AppConfig }).config;
}

describe("FilingDateCard", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("states what each policy means for the user, not its wire value", () => {
    renderCard();

    expect(
      screen.getByText("Today's work is filed for tomorrow's standup."),
    ).toBeDefined();
    expect(
      screen.getByText("Today's work is filed for today's standup."),
    ).toBeDefined();
    // Never the internal name.
    expect(screen.queryByText(/next_business_day/)).toBeNull();
    expect(screen.queryByText(/same_day/)).toBeNull();
  });

  it("says weekend work accumulates into Monday under either policy", () => {
    // Both options roll a weekend forward, so the rule cannot be attached to
    // whichever one happens to be selected.
    renderCard();

    expect(
      screen.getByText(/weekend work accumulates into Monday's file/),
    ).toBeDefined();
  });

  it("marks the App Script's rule as the original and preselects it", () => {
    renderCard();

    const original = screen.getByRole("radio", { name: /Next business day/ });
    expect(original.getAttribute("aria-checked")).toBe("true");
    expect(within(original).getByText("Original")).toBeDefined();
    expect(
      screen
        .getByRole("radio", { name: /Same day/ })
        .getAttribute("aria-checked"),
    ).toBe("false");
  });

  it("persists the chosen policy through set_config", async () => {
    renderCard();

    fireEvent.click(screen.getByRole("radio", { name: /Same day/ }));

    await waitFor(() => {
      expect(lastSavedConfig()?.dates).toEqual({ archive_mode: "same_day" });
    });
  });

  it("does not write when the selected policy is clicked again", async () => {
    renderCard();

    fireEvent.click(screen.getByRole("radio", { name: /Next business day/ }));

    await waitFor(() => {
      expect(screen.getByText(/Right now: work done on/)).toBeDefined();
    });
    expect(lastSavedConfig()).toBeNull();
  });

  it("shows which file today's work lands in right now", async () => {
    renderCard();

    await waitFor(() => {
      expect(screen.getByText("2026-08-04.md")).toBeDefined();
    });
    expect(screen.getByText(/work done on/)).toBeDefined();
  });

  it("follows the backend when the policy is already same-day", async () => {
    renderCard(withArchiveMode(BASE_CONFIG, "same_day"));

    await waitFor(() => {
      expect(screen.getByText("2026-08-03.md")).toBeDefined();
    });
    expect(
      screen.getByRole("radio", { name: /Same day/ }).getAttribute("aria-checked"),
    ).toBe("true");
  });

  it("reports a config it could not load instead of a blank card", async () => {
    mockInvokeCommands({
      get_config: () => {
        throw { code: "config", message: "config store is corrupt" };
      },
    });
    renderWithProviders(<FilingDateCard />);

    await waitFor(() => {
      expect(screen.getByText("Could not load settings.")).toBeDefined();
    });
  });
});

describe("withArchiveMode", () => {
  it("writes a whole dates block onto a config that has none", () => {
    // A `config.json` from before the policy existed deserializes with the
    // default on the Rust side, but a UI that spread `config.dates` here would
    // still be spreading `undefined` and could produce `{}`.
    const withoutDates: Record<string, unknown> = { ...BASE_CONFIG };
    withoutDates.dates = undefined;

    expect(
      withArchiveMode(withoutDates as unknown as AppConfig, "same_day").dates,
    ).toEqual({ archive_mode: "same_day" });
  });

  it("leaves every other field alone", () => {
    const next = withArchiveMode(BASE_CONFIG, "same_day");

    expect(next.dailies_dir).toBe(BASE_CONFIG.dailies_dir);
    expect(next.llm).toBe(BASE_CONFIG.llm);
    expect(next.scheduler).toBe(BASE_CONFIG.scheduler);
  });
});
