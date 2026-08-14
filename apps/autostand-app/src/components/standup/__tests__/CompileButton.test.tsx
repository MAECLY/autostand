/**
 * The quota pre-flight around Compile now.
 *
 * Three rules are under test and none of them is cosmetic:
 *
 * 1. A healthy provider never sees a dialog — the common path stays one click.
 * 2. A provider under the *configured* threshold states the fact and offers the
 *    alternative, but never blocks: "Compile anyway" always renders the standup.
 * 3. Switching provider saves the new order *before* compiling, or the render
 *    would read the old order off disk and use the provider the user declined.
 */

import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { CompileButton } from "@/components/standup/CompileButton";
import { configKey } from "@/hooks/use-config";
import { providerHealthKey } from "@/hooks/use-providers";
import type { AppConfig, ProviderHealth } from "@/lib/types";
import {
  FIXTURE_DATE,
  invokedCommands,
  makeAppConfig,
  makePipelineStatus,
  makeProviderHealth,
  makeUsageWindow,
  mockInvoke,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import { createTestQueryClient, renderWithProviders } from "@/test/render";

const FIVE_HOURS_MS = 5 * 60 * 60 * 1000;

function preview() {
  return {
    token: "tok-1",
    date: FIXTURE_DATE,
    host: "mbp-miguel",
    current_auto: "- old",
    candidate_auto: "- new",
    base_hash: "sha256:abc",
    expires_at: "2026-08-13T13:00:00.000Z",
    render_used: "llm" as const,
    fellback: false,
    message: "ready",
  };
}

function chainConfig(order: string[]): AppConfig {
  const config = makeAppConfig();
  config.llm.provider_order = order;
  config.llm.fallback_enabled = true;
  return config;
}

function lowClaude(): ProviderHealth {
  return makeProviderHealth({
    provider: "claude",
    availability: "low",
    windows: [
      makeUsageWindow({
        id: "five_hour",
        remaining_percent: 12,
        period_duration_ms: FIVE_HOURS_MS,
        runs_out_in_seconds: 2_100,
      }),
    ],
  });
}

function renderCompile(health: ProviderHealth[], config: AppConfig = makeAppConfig()) {
  mockInvokeCommands({
    get_config: () => config,
    set_config: () => undefined,
    get_pipeline_status: () => makePipelineStatus(),
    get_provider_health: () => health,
    preview_regeneration: () => preview(),
    apply_regeneration: () => ({
      date: FIXTURE_DATE,
      host: "mbp-miguel",
      resolution: "use_candidate",
      message: "done",
    }),
  });
  // Seeded rather than fetched: the pre-flight decision is made at click time,
  // so a test that clicked before the queries resolved would exercise the
  // no-data path no matter what fixture it passed.
  const queryClient = createTestQueryClient();
  queryClient.setQueryData(configKey, config);
  queryClient.setQueryData(providerHealthKey, health);
  return renderWithProviders(<CompileButton date={FIXTURE_DATE} />, { queryClient });
}

async function clickCompile() {
  const button = await screen.findByRole("button", { name: /compile now/i });
  fireEvent.click(button);
}

/** The pre-flight only — the regeneration review is a dialog too. */
function preflight(): HTMLElement | null {
  return screen.queryByRole("dialog", { name: /is running low/i });
}

async function findPreflight(): Promise<HTMLElement> {
  return screen.findByRole("dialog", { name: /is running low/i });
}

beforeEach(() => {
  resetTauriMocks();
});

describe("CompileButton pre-flight", () => {
  it("compiles straight through when nothing is under pressure", async () => {
    renderCompile([
      makeProviderHealth({ windows: [makeUsageWindow({ remaining_percent: 90 })] }),
    ]);
    await clickCompile();

    await waitFor(() => {
      expect(invokedCommands()).toContain("preview_regeneration");
    });
    expect(preflight()).toBeNull();
  });

  /** Unknown usage is not bad news — it must not gate a compile. */
  it("compiles straight through when the provider was never measured", async () => {
    renderCompile([makeProviderHealth({ windows: [makeUsageWindow()] })]);
    await clickCompile();

    await waitFor(() => {
      expect(invokedCommands()).toContain("preview_regeneration");
    });
    expect(preflight()).toBeNull();
  });

  it("states the fact and the projection before compiling on a low provider", async () => {
    renderCompile([lowClaude()], chainConfig(["claude", "openai"]));
    await clickCompile();

    const dialog = await findPreflight();
    expect(dialog).toHaveTextContent(
      "12% of the 5 h window left, projected to run out in ~35 min.",
    );
    // Informational: nothing was rendered while the dialog is open.
    expect(invokedCommands()).not.toContain("preview_regeneration");
  });

  it("omits the projection sentence rather than inventing one", async () => {
    const health = makeProviderHealth({
      windows: [
        makeUsageWindow({
          id: "five_hour",
          remaining_percent: 12,
          period_duration_ms: FIVE_HOURS_MS,
        }),
      ],
    });
    renderCompile([health], chainConfig(["claude", "openai"]));
    await clickCompile();

    const dialog = await findPreflight();
    expect(dialog).toHaveTextContent("12% of the 5 h window left.");
    expect(dialog).not.toHaveTextContent("run out");
  });

  it("never blocks: compile anyway renders on the provider the user chose", async () => {
    renderCompile([lowClaude()], chainConfig(["claude", "openai"]));
    await clickCompile();

    fireEvent.click(await screen.findByRole("button", { name: /compile anyway/i }));

    await waitFor(() => {
      expect(invokedCommands()).toContain("preview_regeneration");
    });
    expect(invokedCommands()).not.toContain("set_config");
  });

  it("saves the new provider order before compiling on the alternative", async () => {
    renderCompile([lowClaude()], chainConfig(["claude", "openai", "grok"]));
    await clickCompile();

    fireEvent.click(await screen.findByRole("button", { name: /use openai instead/i }));

    await waitFor(() => {
      expect(invokedCommands()).toContain("preview_regeneration");
    });
    const saved = mockInvoke.mock.calls.find((call) => call[0] === "set_config");
    const config = (saved?.[1] as { config: AppConfig }).config;
    expect(config.llm.provider_order).toEqual(["openai", "claude", "grok"]);
    expect(config.llm.preferred_provider).toBe("openai");
    // Order matters: a compile that overtook the save would read the old chain.
    const commands = invokedCommands();
    expect(commands.indexOf("set_config")).toBeLessThan(
      commands.indexOf("preview_regeneration"),
    );
  });

  it("offers no switch when every other provider is worse off", async () => {
    renderCompile(
      [
        lowClaude(),
        makeProviderHealth({ provider: "openai", availability: "auth_required" }),
      ],
      chainConfig(["claude", "openai"]),
    );
    await clickCompile();

    const dialog = await findPreflight();
    expect(dialog).toHaveTextContent("No other configured provider is in better shape");
    expect(screen.queryByRole("button", { name: /instead/i })).not.toBeInTheDocument();
  });

  it("cancelling leaves the standup untouched", async () => {
    renderCompile([lowClaude()], chainConfig(["claude", "openai"]));
    await clickCompile();

    fireEvent.click(await screen.findByRole("button", { name: /^cancel$/i }));

    await waitFor(() => {
      expect(preflight()).toBeNull();
    });
    expect(invokedCommands()).not.toContain("preview_regeneration");
  });

  /** The threshold is the user's, not a constant baked into the button. */
  it("follows the configured threshold in both directions", async () => {
    const config = chainConfig(["claude", "openai"]);
    config.notifications.low_usage_threshold_percent = 5;
    renderCompile([lowClaude()], config);
    await clickCompile();

    await waitFor(() => {
      expect(invokedCommands()).toContain("preview_regeneration");
    });
    expect(preflight()).toBeNull();
  });
});
