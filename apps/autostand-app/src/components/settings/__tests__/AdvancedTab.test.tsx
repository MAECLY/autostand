import { fireEvent, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import { AdvancedTab } from "@/components/settings/AdvancedTab";
import { useUiStore } from "@/lib/store";
import { resetTauriMocks } from "@/test/mocks";
import { renderWithProviders } from "@/test/render";

const initialState = useUiStore.getState();

const audit = () => screen.getByRole("switch", { name: /show audit/i });
const debug = () => screen.getByRole("switch", { name: /show debug/i });

beforeEach(() => {
  resetTauriMocks();
  useUiStore.setState(initialState, true);
});

describe("AdvancedTab", () => {
  it("starts with both diagnostic screens hidden", () => {
    renderWithProviders(<AdvancedTab />);

    expect(audit()).not.toBeChecked();
    expect(debug()).not.toBeChecked();
  });

  it("switches Audit on its own", () => {
    renderWithProviders(<AdvancedTab />);

    fireEvent.click(audit());

    expect(useUiStore.getState().showAuditNav).toBe(true);
    expect(useUiStore.getState().showDebugNav).toBe(false);
  });

  it("switches Debug on its own", () => {
    renderWithProviders(<AdvancedTab />);

    fireEvent.click(debug());

    expect(useUiStore.getState().showDebugNav).toBe(true);
    expect(useUiStore.getState().showAuditNav).toBe(false);
  });

  it("reflects preferences restored from a previous session", () => {
    useUiStore.getState().setShowDebugNav(true);
    renderWithProviders(<AdvancedTab />);

    expect(audit()).not.toBeChecked();
    expect(debug()).toBeChecked();
  });

  // Someone reading the toggle should not have to guess whether turning it off
  // breaks a link they saved.
  it("says the routes keep working while hidden", () => {
    renderWithProviders(<AdvancedTab />);
    expect(screen.getByText(/keep working while hidden/i)).toBeInTheDocument();
  });
});
