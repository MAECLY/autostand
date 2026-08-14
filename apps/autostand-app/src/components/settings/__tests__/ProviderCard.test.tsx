import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import { ProviderCard, type ProviderCardProps } from "@/components/settings/ProviderCard";
import type { LlmProviderConfig } from "@/lib/types";
import { mockInvokeCommands, resetTauriMocks } from "@/test/mocks";
import { renderWithProviders } from "@/test/render";

function makeProvider(overrides: Partial<LlmProviderConfig> = {}): LlmProviderConfig {
  return {
    id: "claude",
    label: "Claude (Anthropic)",
    enabled: true,
    mode: "CliFirst",
    model: "claude-sonnet-4",
    cli: { found: true, path: "/usr/local/bin/claude", version: "1.0.0" },
    api_key: { set: true, mode: "keychain" },
    ...overrides,
  };
}

function renderCard(
  models: string[] = [],
  overrides: Partial<LlmProviderConfig> = {},
  cardProps: Partial<ProviderCardProps> = {},
) {
  mockInvokeCommands({
    list_provider_models: () => models,
  });
  const onSetModel = vi.fn();
  const onMoveUp = vi.fn();
  const onMoveDown = vi.fn();
  const view = renderWithProviders(
    <ProviderCard
      provider={makeProvider(overrides)}
      timeoutSecs={180}
      isPreferred
      canMoveUp={false}
      canMoveDown
      onSetPreferred={vi.fn()}
      onSetEnabled={vi.fn()}
      onMoveUp={onMoveUp}
      onMoveDown={onMoveDown}
      onSetMode={vi.fn()}
      onSetModel={onSetModel}
      onSetTimeout={vi.fn()}
      onTest={vi.fn().mockResolvedValue({ ok: true, message: "ok", latency_ms: 12 })}
      onSaveKey={vi.fn().mockResolvedValue(undefined)}
      {...cardProps}
    />,
  );
  return { ...view, onSetModel, onMoveUp, onMoveDown };
}

beforeEach(() => {
  resetTauriMocks();
});

describe("ProviderCard model field", () => {
  it("shows only meaningful controls for Built-in Local AI", async () => {
    renderCard(["gemma3:1b"], {
      id: "builtin-local",
      label: "Built-in Local AI",
      mode: "CliOnly",
      model: "gemma3:1b",
      api_key: { set: false, mode: "none" },
    });

    expect(await screen.findByText("gemma3:1b")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Test local AI" })).toBeInTheDocument();
    expect(screen.queryByText("Mode")).toBeNull();
    expect(screen.queryByRole("button", { name: "Store key" })).toBeNull();
  });

  it("keeps a free-text model input when the probe returns nothing", async () => {
    const { onSetModel } = renderCard([]);

    const input = await screen.findByLabelText("Model");
    expect(input).toHaveAttribute("id", "provider-claude-model");
    expect(input).toHaveValue("claude-sonnet-4");
    expect(screen.queryByRole("combobox", { name: "Model" })).toBeNull();

    fireEvent.change(input, { target: { value: "claude-opus-4" } });
    fireEvent.blur(input);

    expect(onSetModel).toHaveBeenCalledWith("claude-opus-4");
  });

  it("renders a Select of discovered models plus Custom…", async () => {
    renderCard(["claude-sonnet-4", "claude-opus-4"]);

    const trigger = await screen.findByRole("combobox", { name: "Model" });
    fireEvent.click(trigger);

    expect(await screen.findByRole("option", { name: "claude-sonnet-4" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "claude-opus-4" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Custom…" })).toBeInTheDocument();
  });

  it("commits a model chosen from the Select", async () => {
    const { onSetModel } = renderCard(["claude-sonnet-4", "claude-opus-4"]);

    fireEvent.click(await screen.findByRole("combobox", { name: "Model" }));
    fireEvent.click(await screen.findByRole("option", { name: "claude-opus-4" }));

    await waitFor(() => expect(onSetModel).toHaveBeenCalledWith("claude-opus-4"));
  });

  it("reveals a custom input when Custom… is chosen", async () => {
    const { onSetModel } = renderCard(["claude-sonnet-4", "claude-opus-4"]);

    fireEvent.click(await screen.findByRole("combobox", { name: "Model" }));
    fireEvent.click(await screen.findByRole("option", { name: "Custom…" }));

    const custom = await screen.findByLabelText("Claude (Anthropic) custom model");
    fireEvent.change(custom, { target: { value: "claude-haiku-4" } });
    fireEvent.blur(custom);

    expect(onSetModel).toHaveBeenCalledWith("claude-haiku-4");
  });
});

describe("ProviderCard collapsed summary", () => {
  it("reports mode and model without being opened", async () => {
    renderCard([], {}, { expanded: false });

    expect(await screen.findByText("CLI first")).toBeInTheDocument();
    expect(screen.getByText("claude-sonnet-4")).toBeInTheDocument();
    // The configure affordance is what the E2E helper drives; keep it named.
    expect(screen.getByRole("button", { name: "Configure" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Model")).toBeNull();
  });

  it("says so rather than showing a blank when no model is set", async () => {
    renderCard([], { model: "" }, { expanded: false });

    expect(await screen.findByText("No model set")).toBeInTheDocument();
  });

  it("swaps the summary for the CLI path once expanded", async () => {
    renderCard([], {}, { expanded: true });

    expect(
      await screen.findByText("/usr/local/bin/claude · 1.0.0"),
    ).toBeInTheDocument();
    // The model now lives in the field, so the summary must not repeat it.
    expect(screen.queryByText("claude-sonnet-4")).toBeNull();
    expect(screen.getByLabelText("Model")).toHaveValue("claude-sonnet-4");
  });

  it("omits the mode summary for Built-in Local AI, which has no transport", async () => {
    renderCard(
      [],
      { id: "builtin-local", label: "Built-in Local AI", mode: "CliOnly", model: "gemma3:1b" },
      { expanded: false },
    );

    expect(await screen.findByText("gemma3:1b")).toBeInTheDocument();
    expect(screen.queryByText("CLI only")).toBeNull();
  });
});

describe("ProviderCard reordering", () => {
  it("keeps the failover controls reachable while collapsed", async () => {
    const { onMoveDown } = renderCard([], {}, { expanded: false });

    const up = await screen.findByRole("button", {
      name: "Move Claude (Anthropic) up",
    });
    const down = screen.getByRole("button", {
      name: "Move Claude (Anthropic) down",
    });

    expect(up).toBeDisabled();
    expect(down).toBeEnabled();

    fireEvent.click(down);
    expect(onMoveDown).toHaveBeenCalledTimes(1);
  });

  it("marks the preferred provider on the card itself", async () => {
    renderCard([], {}, { expanded: false });

    expect(await screen.findByText("Preferred")).toBeInTheDocument();
    expect(
      screen.getByRole("radio", { name: "Prefer Claude (Anthropic)" }),
    ).toBeChecked();
  });
});
