import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("@/test/mocks")).tauriEventMock());

import { fireEvent, screen } from "@testing-library/react";

import {
  StandupReadinessAlert,
  readinessProblems,
} from "@/components/settings/StandupReadinessAlert";
import { standupReadinessKey } from "@/hooks/use-readiness";
import { mockInvokeCommands, resetTauriMocks } from "@/test/mocks";
import { createTestQueryClient, renderWithProviders } from "@/test/render";
import type { StandupReadiness } from "@/lib/types";

function makeReadiness(
  overrides: Partial<StandupReadiness> = {},
): StandupReadiness {
  return {
    github_dir: "/Users/tester/Github",
    github_dir_exists: true,
    repo_count: 4,
    configured_authors: ["dev@x.invalid"],
    git_identity: "machine@x.invalid",
    effective_authors: ["dev@x.invalid"],
    author_source: "configured",
    ready: true,
    ...overrides,
  };
}

/** The state `AppConfig::default()` leaves a fresh install in. */
function makeColdStart(): StandupReadiness {
  return makeReadiness({
    github_dir: "/Users/tester/Documents/Github",
    github_dir_exists: false,
    repo_count: 0,
    configured_authors: [],
    git_identity: null,
    effective_authors: [],
    author_source: "none",
    ready: false,
  });
}

function renderAlert(readiness: StandupReadiness | undefined, onFix?: () => void) {
  mockInvokeCommands({
    get_standup_readiness: () => readiness ?? Promise.reject(new Error("probe failed")),
  });
  const queryClient = createTestQueryClient();
  if (readiness !== undefined) {
    queryClient.setQueryData(standupReadinessKey, readiness);
  }
  return renderWithProviders(<StandupReadinessAlert onFix={onFix} />, {
    queryClient,
  });
}

describe("readinessProblems", () => {
  it("finds nothing wrong with a configured machine", () => {
    expect(readinessProblems(makeReadiness())).toEqual([]);
  });

  /**
   * A cold start fails every precondition at once, but a missing scan root is
   * also *why* the repo count is zero — reporting both reads as two unrelated
   * problems to chase.
   */
  it("reports the missing scan root instead of the repo count it explains", () => {
    const ids = readinessProblems(makeColdStart()).map((problem) => problem.id);

    expect(ids).toEqual(["github-dir", "authors"]);
  });

  it("reports an empty scan root that does exist", () => {
    const problems = readinessProblems(
      makeReadiness({ repo_count: 0 }),
    );

    expect(problems.map((problem) => problem.id)).toEqual(["repos"]);
    expect(problems[0].detail).toContain("/Users/tester/Github");
  });

  /** The git-identity fallback is a working configuration, not a problem. */
  it("accepts the machine git identity as an author filter", () => {
    const problems = readinessProblems(
      makeReadiness({
        configured_authors: [],
        effective_authors: ["machine@x.invalid"],
        author_source: "git-identity",
      }),
    );

    expect(problems).toEqual([]);
  });

  it("reports an author filter nothing can be derived for", () => {
    const problems = readinessProblems(
      makeReadiness({
        configured_authors: [],
        git_identity: null,
        effective_authors: [],
        author_source: "none",
      }),
    );

    expect(problems.map((problem) => problem.id)).toEqual(["authors"]);
  });
});

describe("StandupReadinessAlert", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("confirms a machine that can gather", () => {
    renderAlert(makeReadiness());

    expect(screen.getByText("Ready to gather commits")).toBeDefined();
    expect(screen.getByText(/4 repositories under/)).toBeDefined();
  });

  it("says the fallback identity is doing the filtering", () => {
    renderAlert(
      makeReadiness({
        configured_authors: [],
        effective_authors: ["machine@x.invalid"],
        author_source: "git-identity",
      }),
    );

    expect(
      screen.getByText(/this machine's git identity — no authors configured/),
    ).toBeDefined();
  });

  it("explains every missing precondition on a cold start", () => {
    renderAlert(makeColdStart());

    expect(screen.getByText("Your standup will come back empty")).toBeDefined();
    expect(screen.getByText(/The GitHub directory does not exist/)).toBeDefined();
    expect(screen.getByText(/No commit author to filter on/)).toBeDefined();
  });

  it("routes the user to the tab that fixes it", () => {
    const onFix = vi.fn();
    renderAlert(makeColdStart(), onFix);

    fireEvent.click(screen.getByRole("button", { name: "Fix in Paths" }));

    expect(onFix).toHaveBeenCalledTimes(1);
  });

  /** A probe that has not answered yet must not accuse the configuration. */
  it("stays silent until the probe answers", () => {
    const { container } = renderAlert(undefined);

    expect(container.textContent).toBe("");
  });
});
