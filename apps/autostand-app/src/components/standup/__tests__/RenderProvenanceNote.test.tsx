import { screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import { RenderProvenanceNote } from "@/components/standup/RenderProvenanceNote";
import type { AuditSidecar, LlmProviderConfig } from "@/lib/types";
import {
  FIXTURE_DATE,
  FIXTURE_HOST,
  FIXTURE_OTHER_HOST,
  invokeCount,
  makeAuditSidecar,
  mockInvoke,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import { renderWithProviders } from "@/test/render";

const PROVIDERS: LlmProviderConfig[] = [
  {
    id: "claude",
    label: "Claude (Anthropic)",
    enabled: true,
    mode: "CliFirst",
    model: "claude-sonnet-4",
    cli: { found: true, path: "/usr/local/bin/claude", version: "1.0.0" },
    api_key: { set: false, mode: "none" },
  },
];

function mount(sidecars: AuditSidecar[], hostSlug = FIXTURE_HOST) {
  mockInvokeCommands({
    list_audit_sidecars: () => sidecars,
    list_llm_providers: () => PROVIDERS,
  });
  return renderWithProviders(
    <RenderProvenanceNote date={FIXTURE_DATE} hostSlug={hostSlug} />,
  );
}

function note() {
  return screen.getByRole("region", { name: "Render provenance" });
}

beforeEach(() => {
  resetTauriMocks();
});

describe("RenderProvenanceNote", () => {
  it("names the provider by its label and the model it used", async () => {
    mount([makeAuditSidecar()]);

    await waitFor(() =>
      expect(note()).toHaveTextContent("Rendered by Claude (Anthropic)"),
    );
    expect(note()).toHaveTextContent("claude-sonnet-4");
  });

  it("falls back to the raw provider id when no provider matches it", async () => {
    mount([makeAuditSidecar({ provider: "unknown-provider" })]);

    await waitFor(() =>
      expect(note()).toHaveTextContent("Rendered by unknown-provider"),
    );
  });

  it("reports a deterministic render without probing the providers", async () => {
    mount([
      makeAuditSidecar({ render_used: "det", provider: null, model: null }),
    ]);

    await waitFor(() =>
      expect(note()).toHaveTextContent(
        "Deterministic render — no AI provider was used.",
      ),
    );
    // Labelling costs a CLI probe per provider; a deterministic render has
    // nothing to label, so it must not pay for one.
    expect(invokeCount("list_llm_providers")).toBe(0);
  });

  it("says whose draft lost when the render fell back", async () => {
    mount([
      makeAuditSidecar({ render_used: "llm_fallback", fellback: true }),
    ]);

    await waitFor(() =>
      expect(note()).toHaveTextContent(
        "Deterministic render — the draft from Claude (Anthropic) was not used.",
      ),
    );
  });

  it("trusts `fellback` even when the sidecar still claims an LLM render", async () => {
    // An older sidecar can carry one signal without the other; crediting the
    // provider for a body it did not write would be the worst of both.
    mount([makeAuditSidecar({ render_used: "llm", fellback: true })]);

    await waitFor(() =>
      expect(note()).toHaveTextContent(
        "Deterministic render — the draft from Claude (Anthropic) was not used.",
      ),
    );
  });

  it("says no provider was available when the fallback had no candidate", async () => {
    mount([
      makeAuditSidecar({
        render_used: "llm_fallback",
        fellback: true,
        provider: null,
        model: null,
      }),
    ]);

    await waitFor(() =>
      expect(note()).toHaveTextContent(
        "Deterministic render — no AI provider was available.",
      ),
    );
  });

  it("renders nothing when this host filed no sidecar", async () => {
    const { container } = mount([
      makeAuditSidecar({ host: FIXTURE_OTHER_HOST }),
    ]);

    await waitFor(() => expect(mockInvoke).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing while the host slug is still unknown", async () => {
    const { container } = mount([makeAuditSidecar()], "");

    await waitFor(() => expect(mockInvoke).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  it("states that the provenance never reaches the standup file", async () => {
    mount([makeAuditSidecar()]);

    await waitFor(() =>
      expect(note()).toHaveTextContent(
        "Shown here only; never written to the standup file.",
      ),
    );
  });
});
