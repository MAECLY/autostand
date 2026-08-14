/**
 * The contract under test is "never invent a value".
 *
 * Each case pins one shape of `UsageWindow` to exactly what the rail is allowed
 * to claim about it: a unit is formatted as that unit, a balance gets no bar,
 * and a window that reported nothing says so instead of rendering 0%.
 */

import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { ProviderUsage } from "@/components/settings/ProviderUsage";
import type { ProviderHealth, UsageWindow } from "@/lib/types";
import { mockInvoke, mockInvokeCommands, resetTauriMocks } from "@/test/mocks";
import { renderWithProviders } from "@/test/render";

/** Frozen so "Resets in 2 hours" is a fact about the fixture, not about today. */
const NOW = new Date("2026-08-13T12:00:00.000Z");

function makeWindow(overrides: Partial<UsageWindow> = {}): UsageWindow {
  return {
    id: "five_hour",
    used_percent: null,
    remaining_percent: null,
    resets_at: null,
    ...overrides,
  };
}

function makeHealth(overrides: Partial<ProviderHealth> = {}): ProviderHealth {
  return {
    provider: "claude",
    availability: "available",
    source: "provider_reported",
    windows: [],
    reason: null,
    checked_at: "2026-08-13T11:58:00.000Z",
    ...overrides,
  };
}

function renderUsage(health: ProviderHealth[]) {
  mockInvokeCommands({
    get_provider_health: () => health,
    refresh_provider_health: () => health,
  });
  return renderWithProviders(<ProviderUsage compact />);
}

/** The row a provider owns, so assertions cannot drift to a neighbour. */
async function providerBlock(provider: string): Promise<HTMLElement> {
  const button = await screen.findByRole("button", {
    name: `Refresh ${provider} usage`,
  });
  const block = button.closest("section");
  if (block === null) throw new Error(`no block for ${provider}`);
  return block;
}

beforeEach(() => {
  resetTauriMocks();
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("ProviderUsage figures", () => {
  it("reads a percentage window as what is left, with a bar", async () => {
    renderUsage([
      makeHealth({
        windows: [makeWindow({ remaining_percent: 82, used_percent: 18 })],
      }),
    ]);

    const block = await providerBlock("claude");
    expect(within(block).getByText("82% left")).toBeInTheDocument();
    expect(
      within(block).getByRole("progressbar", { name: "Five Hour remaining" }),
    ).toHaveAttribute("aria-valuenow", "82");
  });

  it("formats a dollar window as dollars, not as a percentage", async () => {
    renderUsage([
      makeHealth({
        windows: [
          makeWindow({
            id: "monthly_spend",
            kind: "consumption",
            unit: "usd",
            used: 12.4,
            limit: 50,
          }),
        ],
      }),
    ]);

    const block = await providerBlock("claude");
    expect(within(block).getByText("$12.40 of $50.00")).toBeInTheDocument();
    // 12.40 of 50 leaves 75.2% — derived from two reported numbers, not guessed.
    expect(within(block).getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "75.2",
    );
  });

  it("counts credits with their noun and thousands separator", async () => {
    renderUsage([
      makeHealth({
        windows: [
          makeWindow({
            id: "wallet",
            kind: "balance",
            unit: "credits",
            available: 1240,
          }),
        ],
      }),
    ]);

    const block = await providerBlock("claude");
    expect(within(block).getByText("1,240 credits left")).toBeInTheDocument();
  });

  it("gives a balance no progress bar — a saldo has no denominator", async () => {
    renderUsage([
      makeHealth({
        windows: [
          makeWindow({
            id: "wallet",
            kind: "balance",
            unit: "usd",
            available: 8.5,
            remaining_percent: 17,
          }),
        ],
      }),
    ]);

    const block = await providerBlock("claude");
    expect(within(block).getByText("$8.50 left")).toBeInTheDocument();
    expect(within(block).queryByRole("progressbar")).toBeNull();
  });

  it("says No data instead of rendering an unreported window as 0%", async () => {
    renderUsage([makeHealth({ windows: [makeWindow({ id: "weekly" })] })]);

    const block = await providerBlock("claude");
    expect(within(block).getByText("Weekly")).toBeInTheDocument();
    expect(within(block).getByText("No data")).toBeInTheDocument();
    expect(within(block).queryByRole("progressbar")).toBeNull();
  });

  it("falls back to used percent when only that was reported", async () => {
    renderUsage([
      makeHealth({ windows: [makeWindow({ used_percent: 30 })] }),
    ]);

    const block = await providerBlock("claude");
    expect(within(block).getByText("30% used")).toBeInTheDocument();
    expect(within(block).getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "70",
    );
  });

  it("prefers the provider's own window label over the derived one", async () => {
    renderUsage([
      makeHealth({
        windows: [
          makeWindow({ id: "five_hour", label: "Sonnet · 5 h", remaining_percent: 50 }),
        ],
      }),
    ]);

    const block = await providerBlock("claude");
    expect(within(block).getByText("Sonnet · 5 h")).toBeInTheDocument();
    expect(within(block).queryByText("Five Hour")).toBeNull();
  });
});

describe("ProviderUsage window context", () => {
  it("states the window length, the reset and the pace on one line", async () => {
    renderUsage([
      makeHealth({
        windows: [
          makeWindow({
            remaining_percent: 40,
            resets_at: "2026-08-13T14:00:00.000Z",
            period_duration_ms: 5 * 60 * 60 * 1000,
            pace: "behind",
          }),
        ],
      }),
    ]);

    const block = await providerBlock("claude");
    expect(within(block).getByText("5 h window")).toBeInTheDocument();
    expect(within(block).getByText("Resets in 2 hours")).toBeInTheDocument();
    expect(within(block).getByText("Runs out before reset")).toBeInTheDocument();
  });

  it("colours the bar by pace", async () => {
    renderUsage([
      makeHealth({
        provider: "claude",
        windows: [makeWindow({ remaining_percent: 90, pace: "ahead" })],
      }),
      makeHealth({
        provider: "grok",
        windows: [makeWindow({ remaining_percent: 20, pace: "behind" })],
      }),
    ]);

    const ahead = within(await providerBlock("claude")).getByRole("progressbar");
    const behind = within(await providerBlock("grok")).getByRole("progressbar");

    expect(ahead.firstElementChild).toHaveClass("bg-success");
    expect(behind.firstElementChild).toHaveClass("bg-warning");
  });

  it("leaves the bar unpainted when no pace was projected", async () => {
    renderUsage([
      makeHealth({ windows: [makeWindow({ remaining_percent: 60 })] }),
    ]);

    const block = await providerBlock("claude");
    const bar = within(block).getByRole("progressbar").firstElementChild;
    expect(bar).not.toHaveClass("bg-success");
    expect(bar).not.toHaveClass("bg-warning");
  });
});

describe("ProviderUsage provider header", () => {
  it("puts the plan next to the name so a percentage means something", async () => {
    renderUsage([makeHealth({ plan: "Max 20x" })]);

    const block = await providerBlock("claude");
    expect(within(block).getByText("Max 20x")).toBeInTheDocument();
    expect(within(block).getByText("Available")).toBeInTheDocument();
  });

  it("marks a cached snapshot and shows the non-fatal notice", async () => {
    renderUsage([
      makeHealth({
        stale: true,
        notice: "Re-login for live usage",
        availability: "unknown",
      }),
    ]);

    const block = await providerBlock("claude");
    expect(within(block).getByText("Cached")).toBeInTheDocument();
    expect(within(block).getByText("Re-login for live usage")).toBeInTheDocument();
  });

  it("omits the cached mark when the reading is fresh", async () => {
    renderUsage([makeHealth({ stale: false })]);

    const block = await providerBlock("claude");
    expect(within(block).queryByText("Cached")).toBeNull();
  });

  it("explains the availability verdict and when it was read", async () => {
    renderUsage([
      makeHealth({ availability: "auth_required", reason: "not_logged_in" }),
    ]);

    const block = await providerBlock("claude");
    expect(within(block).getByText("Sign-in required")).toBeInTheDocument();
    expect(
      within(block).getByText("Sign in with the provider CLI, then refresh."),
    ).toBeInTheDocument();
    expect(within(block).getByText("Checked 2 minutes ago")).toBeInTheDocument();
  });
});

describe("ProviderUsage refresh", () => {
  it("refreshes one provider without touching the others", async () => {
    renderUsage([makeHealth(), makeHealth({ provider: "grok" })]);

    fireEvent.click(
      await screen.findByRole("button", { name: "Refresh grok usage" }),
    );

    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("refresh_provider_health", {
        provider: "grok",
      }),
    );
  });

  it("refreshes every provider from the panel header", async () => {
    renderUsage([makeHealth()]);

    fireEvent.click(await screen.findByRole("button", { name: "Refresh all" }));

    // `null` is the backend's "every provider" selector, not a missing argument.
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("refresh_provider_health", {
        provider: null,
      }),
    );
  });

  it("invites the user to add a provider when none is configured", async () => {
    renderUsage([]);

    expect(
      await screen.findByText(
        "No provider is configured yet. Add one on the left to track its quota.",
      ),
    ).toBeInTheDocument();
  });
});
