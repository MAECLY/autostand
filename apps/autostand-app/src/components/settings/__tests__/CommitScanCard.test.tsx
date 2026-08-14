import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import { fireEvent, screen, waitFor } from "@testing-library/react";

import { CommitScanCard } from "@/components/settings/CommitScanCard";
import { configKey } from "@/hooks/use-config";
import { standupReadinessKey } from "@/hooks/use-readiness";
import { makeAppConfig, mockInvoke, mockInvokeCommands, resetTauriMocks } from "@/test/mocks";
import { createTestQueryClient, renderWithProviders } from "@/test/render";
import type { AppConfig, StandupReadiness } from "@/lib/types";

const IDENTITY = "machine@example.invalid";

function makeReadiness(
  overrides: Partial<StandupReadiness> = {},
): StandupReadiness {
  return {
    github_dir: "/Users/tester/Github",
    github_dir_exists: true,
    repo_count: 3,
    configured_authors: ["Tester"],
    git_identity: IDENTITY,
    effective_authors: ["Tester"],
    author_source: "configured",
    ready: true,
    ...overrides,
  };
}

interface RenderOptions {
  config?: AppConfig;
  readiness?: StandupReadiness;
}

function renderCard({
  config = makeAppConfig(),
  readiness = makeReadiness(),
}: RenderOptions = {}) {
  mockInvokeCommands({
    get_config: () => config,
    set_config: () => undefined,
    get_standup_readiness: () => readiness,
  });
  const queryClient = createTestQueryClient();
  queryClient.setQueryData(configKey, config);
  queryClient.setQueryData(standupReadinessKey, readiness);
  return renderWithProviders(<CommitScanCard />, { queryClient });
}

/** The `standup_authors` argument of the last `set_config` call. */
function savedAuthors(): string[] {
  const call = mockInvoke.mock.calls
    .filter(([command]) => command === "set_config")
    .at(-1);
  const args = call?.[1] as { config: AppConfig } | undefined;
  return args?.config.standup_authors ?? [];
}

describe("CommitScanCard", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("lists the configured authors", () => {
    renderCard({ config: makeAppConfig({ standup_authors: ["a@x.invalid", "b@x.invalid"] }) });

    expect(screen.getByText("a@x.invalid")).toBeDefined();
    expect(screen.getByText("b@x.invalid")).toBeDefined();
  });

  /**
   * The cascade's second step. An empty list is not an error on a machine that
   * has a git identity, but the user has to be told which identity is standing
   * in for the list they never filled.
   */
  it("names the machine git identity when no author is configured", () => {
    renderCard({
      config: makeAppConfig({ standup_authors: [] }),
      readiness: makeReadiness({
        configured_authors: [],
        effective_authors: [IDENTITY],
        author_source: "git-identity",
      }),
    });

    expect(screen.getByText(/Falling back to this machine/)).toBeDefined();
    expect(screen.getAllByText(IDENTITY).length).toBeGreaterThan(0);
  });

  /** The cascade's third step: local-git refuses to gather at all. */
  it("warns when neither an author nor a git identity exists", () => {
    renderCard({
      config: makeAppConfig({ standup_authors: [] }),
      readiness: makeReadiness({
        configured_authors: [],
        git_identity: null,
        effective_authors: [],
        author_source: "none",
        ready: false,
      }),
    });

    expect(screen.getByText("No author to filter on")).toBeDefined();
    expect(screen.queryByText(/Falling back to this machine/)).toBeNull();
  });

  it("saves an author added through the form", async () => {
    renderCard({ config: makeAppConfig({ standup_authors: ["a@x.invalid"] }) });

    fireEvent.change(screen.getByLabelText("Add a commit author"), {
      target: { value: "  new@x.invalid  " },
    });
    fireEvent.click(screen.getByRole("button", { name: /Add/ }));
    fireEvent.click(screen.getByRole("button", { name: /Save commit scan/ }));

    await waitFor(() => {
      expect(savedAuthors()).toEqual(["a@x.invalid", "new@x.invalid"]);
    });
  });

  it("refuses to add a blank or duplicate author", () => {
    renderCard({ config: makeAppConfig({ standup_authors: ["a@x.invalid"] }) });
    const input = screen.getByLabelText("Add a commit author");

    fireEvent.change(input, { target: { value: "   " } });
    expect(screen.getByRole("button", { name: /Add/ }).getAttribute("disabled")).not.toBeNull();

    fireEvent.change(input, { target: { value: "a@x.invalid" } });
    fireEvent.click(screen.getByRole("button", { name: /Add/ }));

    expect(screen.getAllByText("a@x.invalid")).toHaveLength(1);
    expect(screen.getByRole("button", { name: /Save commit scan/ }).getAttribute("disabled")).not.toBeNull();
  });

  it("removes an author", async () => {
    renderCard({ config: makeAppConfig({ standup_authors: ["a@x.invalid", "b@x.invalid"] }) });

    fireEvent.click(screen.getByRole("button", { name: "Remove a@x.invalid" }));
    fireEvent.click(screen.getByRole("button", { name: /Save commit scan/ }));

    await waitFor(() => {
      expect(savedAuthors()).toEqual(["b@x.invalid"]);
    });
  });

  it("offers the machine identity as a one-click suggestion", async () => {
    renderCard({ config: makeAppConfig({ standup_authors: ["a@x.invalid"] }) });

    fireEvent.click(
      screen.getByRole("button", { name: new RegExp(IDENTITY) }),
    );
    fireEvent.click(screen.getByRole("button", { name: /Save commit scan/ }));

    await waitFor(() => {
      expect(savedAuthors()).toEqual(["a@x.invalid", IDENTITY]);
    });
  });

  it("does not suggest an identity that is already in the list", () => {
    renderCard({ config: makeAppConfig({ standup_authors: [IDENTITY] }) });

    expect(screen.queryByRole("button", { name: /Use this machine/ })).toBeNull();
  });

  /** `git_refs` is advanced: reachable, but never in the way. */
  it("keeps the git refs field behind the advanced disclosure", () => {
    renderCard({ config: makeAppConfig({ git_refs: "--branches" }) });

    expect(screen.queryByLabelText("Git refs")).toBeNull();

    fireEvent.click(screen.getByText("Advanced: scanned refs"));

    const refs = screen.getByLabelText("Git refs") as HTMLInputElement;
    expect(refs.value).toBe("--branches");
    expect(refs.placeholder).toBe("--all");
  });

  it("saves an edited ref selector", async () => {
    renderCard({ config: makeAppConfig({ git_refs: "--branches" }) });

    fireEvent.click(screen.getByText("Advanced: scanned refs"));
    fireEvent.change(screen.getByLabelText("Git refs"), {
      target: { value: "  --branches --tags  " },
    });
    fireEvent.click(screen.getByRole("button", { name: /Save commit scan/ }));

    await waitFor(() => {
      const call = mockInvoke.mock.calls
        .filter(([command]) => command === "set_config")
        .at(-1);
      const args = call?.[1] as { config: AppConfig } | undefined;
      expect(args?.config.git_refs).toBe("--branches --tags");
    });
  });

  /** An unanswered probe must not accuse a machine of having no identity. */
  it("does not claim a missing git identity before the probe answers", () => {
    mockInvokeCommands({
      get_config: () => makeAppConfig({ standup_authors: [] }),
      set_config: () => undefined,
      get_standup_readiness: () => new Promise(() => undefined),
    });
    const queryClient = createTestQueryClient();
    queryClient.setQueryData(configKey, makeAppConfig({ standup_authors: [] }));
    renderWithProviders(<CommitScanCard />, { queryClient });

    expect(screen.getByText("No authors configured")).toBeDefined();
    expect(screen.queryByText("No author to filter on")).toBeNull();
  });

  it("keeps Save disabled until something changes", () => {
    renderCard();

    expect(
      screen.getByRole("button", { name: /Save commit scan/ }).getAttribute("disabled"),
    ).not.toBeNull();
  });
});
