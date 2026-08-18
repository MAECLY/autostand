/**
 * The dialog's whole job is knowing when *not* to appear.
 *
 * It sits at the shell, so it is mounted on every screen for the life of the
 * app. Opening it when nothing is wrong would be a modal in front of the product
 * on every launch, which is how people learn to dismiss dialogs unread.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SystemAccessDialog } from "@/components/access/SystemAccessDialog";
import type { SystemAccess } from "@/lib/types";

const getSystemAccess = vi.fn<() => Promise<SystemAccess>>();

vi.mock("@/lib/tauri", () => ({
  tauriApi: {
    getSystemAccess: () => getSystemAccess(),
    requestSystemAccess: () => getSystemAccess(),
    openAccessSettings: () => Promise.resolve(),
  },
}));

function access(overrides: Partial<SystemAccess> = {}): SystemAccess {
  return {
    platform: "macos",
    gated: true,
    needs_attention: false,
    settings_url: "x-apple.systempreferences:com.apple.preference.security",
    checks: [],
    ...overrides,
  };
}

function renderDialog() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <SystemAccessDialog />
    </QueryClientProvider>,
  );
}

describe("SystemAccessDialog", () => {
  beforeEach(() => {
    getSystemAccess.mockReset();
  });

  it("stays closed when every folder is readable", async () => {
    getSystemAccess.mockResolvedValue(
      access({
        checks: [
          {
            id: "dailies-dir",
            label: "Standup folder",
            reason: "",
            path: "/Users/tester/Sync",
            state: "granted",
          },
        ],
      }),
    );

    renderDialog();

    await waitFor(() => expect(getSystemAccess).toHaveBeenCalled());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("stays closed on an OS that does not gate folder access", async () => {
    getSystemAccess.mockResolvedValue(
      access({ platform: "linux", gated: false, needs_attention: false }),
    );

    renderDialog();

    await waitFor(() => expect(getSystemAccess).toHaveBeenCalled());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("names the refused folder and why it is read", async () => {
    getSystemAccess.mockResolvedValue(
      access({
        needs_attention: true,
        checks: [
          {
            id: "github-dir",
            label: "Repository folder",
            reason: "Scanned for git repositories.",
            path: "/Users/tester/Documents/Github",
            state: "denied",
          },
          {
            id: "dailies-dir",
            label: "Standup folder",
            reason: "Where standups are written.",
            path: "/Users/tester/Sync",
            state: "granted",
          },
        ],
      }),
    );

    renderDialog();

    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText("Repository folder")).toBeInTheDocument();
    expect(
      screen.getByText("/Users/tester/Documents/Github"),
    ).toBeInTheDocument();
    expect(screen.getByText("Scanned for git repositories.")).toBeInTheDocument();

    // The granted one is not listed: this dialog is a to-do list, and a line
    // that needs nothing done is noise in front of the ones that do.
    expect(screen.queryByText("Standup folder")).not.toBeInTheDocument();
  });
});
