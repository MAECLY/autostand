/**
 * "Why is my standup empty?", answered before the user has to ask.
 *
 * A fresh install carries `github_dir: ""` and `standup_authors: []`, so the
 * authoritative Local Git source has nowhere to look and nobody to look for.
 * Until now that combination produced a standup with an empty FACTS block and
 * no explanation anywhere in the UI. This banner states the three preconditions
 * — a scan root that exists, repositories under it, an author filter — and
 * which of them is missing.
 *
 * The verdict is computed by the backend (`get_standup_readiness`) from the same
 * helpers `local-git` uses, so it cannot claim a readiness the pipeline does not
 * have.
 */

import { CircleCheck, TriangleAlert } from "lucide-react";

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@autostand/ui/components/alert";
import { Button } from "@autostand/ui/components/button";

import { useStandupReadiness } from "@/hooks/use-readiness";
import type { StandupReadiness } from "@/lib/types";

export interface ReadinessProblem {
  id: "github-dir" | "repos" | "authors";
  title: string;
  detail: string;
}

/**
 * The preconditions Local Git fails, worst first.
 *
 * A missing scan root also means zero repos, so only the root is reported —
 * two rows for one cause read as two separate problems to fix.
 */
export function readinessProblems(
  readiness: StandupReadiness,
): ReadinessProblem[] {
  const problems: ReadinessProblem[] = [];

  if (!readiness.github_dir_exists) {
    problems.push({
      id: "github-dir",
      title: "The GitHub directory does not exist",
      detail: `Autostand looks for repositories in ${readiness.github_dir}. Point it at the folder your repos live in.`,
    });
  } else if (readiness.repo_count === 0) {
    problems.push({
      id: "repos",
      title: "No repositories to scan",
      detail: `${readiness.github_dir} contains no git repositories. Local Git scans that folder one level deep, so each repo has to sit directly inside it.`,
    });
  }

  if (readiness.author_source === "none") {
    problems.push({
      id: "authors",
      title: "No commit author to filter on",
      detail:
        "Commit authors is empty and this machine has no git config user.email to fall back on, so Local Git cannot tell your commits from your teammates'.",
    });
  }

  return problems;
}

/** One-line summary of the author filter that will be applied. */
function authorSummary(readiness: StandupReadiness): string {
  const authors = readiness.effective_authors.join(", ");
  return readiness.author_source === "git-identity"
    ? `${authors} (this machine's git identity — no authors configured)`
    : authors;
}

export interface StandupReadinessAlertProps {
  /** Called by the "Fix this" button; typically switches to the Paths tab. */
  onFix?: () => void;
}

export function StandupReadinessAlert({ onFix }: StandupReadinessAlertProps) {
  const readiness = useStandupReadiness();

  // A failed or in-flight probe is not a verdict on the configuration: staying
  // silent beats claiming a problem that may not exist.
  if (readiness.data === undefined) return null;

  const problems = readinessProblems(readiness.data);

  if (problems.length === 0) {
    return (
      <Alert variant="success">
        <CircleCheck />
        <AlertTitle>Ready to gather commits</AlertTitle>
        <AlertDescription>
          {readiness.data.repo_count} repositories under{" "}
          <code className="font-mono">{readiness.data.github_dir}</code>,
          filtered by {authorSummary(readiness.data)}.
        </AlertDescription>
      </Alert>
    );
  }

  return (
    <Alert variant="warning">
      <TriangleAlert />
      <AlertTitle>Your standup will come back empty</AlertTitle>
      <AlertDescription className="flex flex-col gap-3">
        <p>
          Local Git is the authoritative source. Until these are fixed it has no
          commits to report:
        </p>
        <ul className="flex list-disc flex-col gap-1 pl-5">
          {problems.map((problem) => (
            <li key={problem.id}>
              <span className="font-medium">{problem.title}.</span>{" "}
              {problem.detail}
            </li>
          ))}
        </ul>
        {onFix !== undefined && (
          <Button
            type="button"
            variant="outline"
            className="self-start"
            onClick={onFix}
          >
            Fix in Paths
          </Button>
        )}
      </AlertDescription>
    </Alert>
  );
}
