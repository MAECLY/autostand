import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import { ProviderCard } from "@/components/settings/ProviderCard";
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
) {
  mockInvokeCommands({
    list_provider_models: () => models,
  });
  const onSetModel = vi.fn();
  const view = renderWithProviders(
    <ProviderCard
      provider={makeProvider(overrides)}
      timeoutSecs={180}
      isPreferred
      onSetPreferred={vi.fn()}
      onSetMode={vi.fn()}
      onSetModel={onSetModel}
      onSetTimeout={vi.fn()}
      onTest={vi.fn().mockResolvedValue({ ok: true, message: "ok", latency_ms: 12 })}
      onSaveKey={vi.fn().mockResolvedValue(undefined)}
    />,
  );
  return { ...view, onSetModel };
}

beforeEach(() => {
  resetTauriMocks();
});

describe("ProviderCard model field", () => {
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
