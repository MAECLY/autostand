import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

import { DependencyChecklist } from "@/components/settings/DependencyChecklist";
import {
  invokeCount,
  mockInvoke,
  mockInvokeCommands,
  resetTauriMocks,
} from "@/test/mocks";
import type { Dependency, Remediation } from "@/lib/types";
import { renderWithProviders } from "@/test/render";

function makeDependency(overrides: Partial<Dependency> = {}): Dependency {
  return {
    id: "repo-sync.git",
    group: "repo_sync",
    label: "Git",
    description: "Commits and pushes the standup history from the sync folder.",
    state: "missing",
    detail: null,
    remediation: null,
    ...overrides,
  };
}

function makeRemediation(overrides: Partial<Remediation> = {}): Remediation {
  return {
    kind: "terminal_command",
    label: "Install with Homebrew",
    command: "brew install git",
    url: "https://git-scm.com/downloads",
    runnable: true,
    note: null,
    ...overrides,
  };
}

function renderChecklist(dependencies: Dependency[]) {
  mockInvokeCommands({
    get_dependency_status: () => dependencies,
    run_dependency_remediation: (args) => ({
      dependency_id: String(args.dependencyId),
      performed: true,
      message: "done",
      dependency: dependencies[0],
    }),
  });
  return renderWithProviders(<DependencyChecklist group="repo_sync" />);
}

async function ready(label: string): Promise<HTMLElement> {
  return waitFor(() => screen.getByText(label));
}

beforeEach(() => {
  resetTauriMocks();
});

describe("DependencyChecklist", () => {
  it("asks the backend only for the group it renders", async () => {
    renderChecklist([makeDependency()]);

    await ready("Git");
    expect(mockInvoke).toHaveBeenCalledWith("get_dependency_status", {
      group: "repo_sync",
    });
  });

  it("states each requirement's verdict in words, not only in colour", async () => {
    renderChecklist([
      makeDependency({ id: "a", label: "Git", state: "ok", detail: "/usr/bin/git" }),
      makeDependency({ id: "b", label: "GitHub CLI", state: "missing" }),
      makeDependency({ id: "c", label: "GitHub sign-in", state: "misconfigured" }),
      makeDependency({ id: "d", label: "Unknowable", state: "unknown" }),
    ]);

    await ready("Ready");
    expect(screen.getByText("Missing")).toBeInTheDocument();
    expect(screen.getByText("Action needed")).toBeInTheDocument();
    expect(screen.getByText("Unknown")).toBeInTheDocument();
    expect(screen.getByText("/usr/bin/git")).toBeInTheDocument();
  });

  // The command has to be readable before anything can run it: that is the
  // whole difference between guidance and a black-box installer.
  it("prints the exact command next to the button that would run it", async () => {
    renderChecklist([makeDependency({ remediation: makeRemediation() })]);

    await ready("brew install git");
    expect(
      screen.getByRole("button", { name: "Copy the command for Git" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run" })).toBeInTheDocument();
    expect(invokeCount("run_dependency_remediation")).toBe(0);
  });

  it("runs a remediation only when the user asks for it", async () => {
    renderChecklist([makeDependency({ remediation: makeRemediation() })]);

    fireEvent.click(await screen.findByRole("button", { name: "Run" }));

    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("run_dependency_remediation", {
        dependencyId: "repo-sync.git",
      }),
    );
  });

  // Signing in opens a browser and waits for a code; a Run button there would
  // hang behind a spinner forever.
  it("offers copy but never Run for a command Autostand must not execute", async () => {
    renderChecklist([
      makeDependency({
        label: "GitHub sign-in",
        remediation: makeRemediation({
          command: "gh auth login --hostname github.com --web",
          runnable: false,
          note: "Run this in your own terminal: it opens a browser to finish sign-in.",
        }),
      }),
    ]);

    await ready("gh auth login --hostname github.com --web");
    expect(screen.queryByRole("button", { name: "Run" })).not.toBeInTheDocument();
    expect(
      screen.getByText(
        "Run this in your own terminal: it opens a browser to finish sign-in.",
      ),
    ).toBeInTheDocument();
  });

  it("opens the official guide when no command can be vouched for", async () => {
    renderChecklist([
      makeDependency({
        remediation: makeRemediation({
          kind: "doc_link",
          label: "Open the Git download page",
          command: null,
          runnable: false,
        }),
      }),
    ]);

    fireEvent.click(
      await screen.findByRole("button", { name: "Open the Git download page" }),
    );

    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("run_dependency_remediation", {
        dependencyId: "repo-sync.git",
      }),
    );
  });

  it("states an in-app step instead of a button that jumps nowhere", async () => {
    renderChecklist([
      makeDependency({
        id: "local-ai.model",
        label: "Downloaded model",
        remediation: makeRemediation({
          kind: "in_app_action",
          label: "Download a model from the list below.",
          command: null,
          url: null,
          runnable: false,
        }),
      }),
    ]);

    await ready("Download a model from the list below.");
    expect(screen.queryByRole("button", { name: "Run" })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Copy the command/ }),
    ).not.toBeInTheDocument();
  });

  it("shows nothing to fix once a requirement is satisfied", async () => {
    renderChecklist([makeDependency({ state: "ok", remediation: null })]);

    await ready("Ready");
    expect(screen.queryByRole("button", { name: "Run" })).not.toBeInTheDocument();
    expect(screen.queryByText(/brew install/)).not.toBeInTheDocument();
  });

  // The probe spawns subprocesses, so it must never poll: Recheck is the only
  // way a mounted checklist re-probes.
  it("re-probes only when Recheck is pressed", async () => {
    renderChecklist([makeDependency()]);

    await ready("Git");
    expect(invokeCount("get_dependency_status")).toBe(1);

    fireEvent.click(screen.getByRole("button", { name: /Recheck/ }));

    await waitFor(() => expect(invokeCount("get_dependency_status")).toBe(2));
  });

  it("reports a failed probe instead of claiming everything is fine", async () => {
    mockInvoke.mockRejectedValue({ code: "io", message: "probe exploded" });
    renderWithProviders(<DependencyChecklist group="local_ai" />);

    await waitFor(() =>
      expect(screen.getByText(/probe exploded/)).toBeInTheDocument(),
    );
  });
});
