/**
 * Classification badge for one audited bullet.
 *
 * The six classes and their match rules come from `docs/specs/audit.md`
 * § Phantom detection; each one owns a `--audit-*` token so the same hue means
 * the same provenance everywhere in the app.
 */

import { Badge } from "@design-system/components/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@design-system/components/tooltip";

import { cn } from "@/lib/utils";

export type AuditClassification =
  | "commit"
  | "github"
  | "review"
  | "note"
  | "phantom"
  | "unverified";

interface ClassificationMeta {
  /** Token class carrying this classification's hue. */
  color: string;
  /** What the badge means — surfaced as the tooltip. */
  hint: string;
}

const CLASSIFICATIONS: Record<AuditClassification, ClassificationMeta> = {
  commit: {
    color: "text-audit-commit",
    hint: "Traces to a git commit in the sidecar's facts — the bullet shares a ticket key or a commit subject.",
  },
  github: {
    color: "text-audit-github",
    hint: "Traces to a pull request in the gathered GitHub block.",
  },
  review: {
    color: "text-audit-review",
    hint: "Traces to a pull request review in the gathered PRREV block.",
  },
  note: {
    color: "text-audit-note",
    hint: "Traces to a note clause that survived the scrub.",
  },
  phantom: {
    color: "text-audit-phantom",
    hint: "Code-change bullet on a forbidden ticket with no matching fact or note. A phantom fails the audit.",
  },
  unverified: {
    color: "text-audit-unverified",
    hint: "No matching source at all. A warning, not a failure — it may be a legitimate note the classifier could not match.",
  },
};

export interface AuditBadgeProps {
  classification: AuditClassification;
  size?: "sm" | "default";
  className?: string;
}

export function AuditBadge({
  classification,
  size = "default",
  className,
}: AuditBadgeProps) {
  const meta = CLASSIFICATIONS[classification];

  return (
    // Self-contained provider: badges appear inside tables and lists that have
    // no tooltip context of their own.
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          {/* A span, not a button: `tabIndex` is what makes the hint reachable
              by keyboard without nesting an interactive element in a row. */}
          <Badge
            variant="outline"
            tabIndex={0}
            className={cn(
              "gap-1.5 font-mono",
              meta.color,
              size === "sm" && "px-2 py-0",
              className,
            )}
          >
            <span aria-hidden className="size-1.5 rounded-full bg-current" />
            {classification}
          </Badge>
        </TooltipTrigger>
        <TooltipContent>{meta.hint}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
