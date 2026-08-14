import { screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { SyncTab } from "@/components/settings/SyncTab";
import { DEPENDENCY_IDS } from "@/hooks/use-dependencies";
import {
  makeAppConfig,
  mockInvoke,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import type { Dependency, RepoSyncStatus } from "@/lib/types";
import { renderWithProviders } from "@/test/render";

const SIGNED_OUT: Dependency[] = [
  {
    id: DEPENDENCY_IDS.git,
    group: "repo_sync",
    label: "Git",
    description: "Commits and pushes the standup history from the sync folder.",
    state: "ok",
    detail: "/usr/bin/git",
    remediation: null,
  },
  {
    id: DEPENDENCY_IDS.gh,
    group: "repo_sync",
    label: "GitHub CLI",
    description: "Creates the private repository and verifies it stayed private.",
    state: "missing",
    detail: null,
    remediation: {
      kind: "terminal_command",
      label: "Install with Homebrew",
      command: "brew install gh",
      url: "https://github.com/cli/cli",
      runnable: true,
      note: null,
    },
  },
];

function makeRepoStatus(overrides: Partial<RepoSyncStatus> = {}): RepoSyncStatus {
  return {
    git_available: true,
    gh_available: false,
    gh_authenticated: false,
    can_setup: false,
    configured: false,
    enabled: false,
    repo_path: "/Users/tester/Sync/Github_Dailies",
    repository: null,
    private: null,
    message: "Repo Sync stays off until its requirements are met.",
    ...overrides,
  };
}

function renderTab(dependencies: Dependency[] = SIGNED_OUT) {
  mockInvokeCommands({
    get_config: () => makeAppConfig(),
    detect_cloud_folders: () => [],
    get_repo_sync_status: () => makeRepoStatus(),
    get_dependency_status: () => dependencies,
  });
  return renderWithProviders(<SyncTab />);
}

beforeEach(() => {
  resetTauriMocks();
});

describe("SyncTab requirements", () => {
  it("replaces the bare status chips with the actionable checklist", async () => {
    renderTab();

    await waitFor(() => expect(screen.getByText("GitHub CLI")).toBeInTheDocument());
    expect(screen.getByText("brew install gh")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("get_dependency_status", {
      group: "repo_sync",
    });
  });

  // The prerequisite is usually why `get_repo_sync_status` failed, so it has to
  // render outside that request's success branch.
  it("still lists requirements when the repo status request fails", async () => {
    mockInvokeCommands({
      get_config: () => makeAppConfig(),
      detect_cloud_folders: () => [],
      get_repo_sync_status: () => {
        throw { code: "git", message: "status unavailable" };
      },
      get_dependency_status: () => SIGNED_OUT,
    });
    renderWithProviders(<SyncTab />);

    await waitFor(() =>
      expect(screen.getByText("status unavailable")).toBeInTheDocument(),
    );
    expect(await screen.findByText("brew install gh")).toBeInTheDocument();
  });
});
