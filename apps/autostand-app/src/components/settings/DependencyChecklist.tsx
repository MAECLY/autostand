/**
 * Shared requirement checklist for a feature's external prerequisites.
 *
 * Repo Sync and Local AI both depend on programs Autostand does not ship, and a
 * bare "Missing" chip left the user to guess the fix. Every unmet requirement
 * here carries exactly one next step, and a command is always printed before it
 * can be run — Autostand never installs anything on its own initiative.
 */

import {
  CircleAlert,
  CircleCheck,
  CircleHelp,
  CircleX,
  ExternalLink,
  RefreshCw,
  type LucideIcon,
} from "lucide-react";

import { Badge, type BadgeProps } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";

import { CopyButton } from "@/components/common/CopyButton";
import {
  useDependencies,
  useRunDependencyRemediation,
} from "@/hooks/use-dependencies";
import { toAppError } from "@/lib/error";
import type { Dependency, DependencyGroup, DependencyState } from "@/lib/types";
import { cn } from "@/lib/utils";

interface StateMeta {
  label: string;
  icon: LucideIcon;
  tone: string;
  badge: BadgeProps["variant"];
}

const STATE_META: Record<DependencyState, StateMeta> = {
  ok: { label: "Ready", icon: CircleCheck, tone: "text-success", badge: "success" },
  missing: {
    label: "Missing",
    icon: CircleX,
    tone: "text-destructive",
    badge: "error",
  },
  misconfigured: {
    label: "Action needed",
    icon: CircleAlert,
    tone: "text-warning",
    badge: "warning",
  },
  unknown: {
    label: "Unknown",
    icon: CircleHelp,
    tone: "text-muted-foreground",
    badge: "secondary",
  },
};

interface RemediationActionProps {
  dependency: Dependency;
  pending: boolean;
  onRun: (dependencyId: string) => void;
}

function RemediationAction({
  dependency,
  pending,
  onRun,
}: RemediationActionProps) {
  const remediation = dependency.remediation;
  if (remediation === null) return null;

  // An in-app step lives in this very screen, so a button that jumps nowhere
  // would be noise: the instruction is the affordance.
  if (remediation.kind === "in_app_action") {
    return (
      <p className="mt-2 text-xs text-muted-foreground">{remediation.label}</p>
    );
  }

  if (remediation.kind === "doc_link") {
    return (
      <div className="mt-2 flex flex-col gap-1">
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="self-start"
          disabled={pending}
          onClick={() => onRun(dependency.id)}
        >
          <ExternalLink aria-hidden="true" /> {remediation.label}
        </Button>
        {remediation.note !== null ? (
          <p className="text-xs text-muted-foreground">{remediation.note}</p>
        ) : null}
      </div>
    );
  }

  const command = remediation.command ?? "";
  return (
    <div className="mt-2 flex flex-col gap-1">
      <div className="flex items-center gap-2 rounded-md bg-inset p-2">
        <code className="min-w-0 flex-1 truncate font-mono text-xs">
          {command}
        </code>
        <CopyButton
          text={command}
          label={`Copy the command for ${dependency.label}`}
          size="icon"
        />
        {remediation.runnable ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={pending}
            onClick={() => onRun(dependency.id)}
          >
            {pending ? "Installing…" : "Run"}
          </Button>
        ) : null}
      </div>
      {remediation.note !== null ? (
        <p className="text-xs text-muted-foreground">{remediation.note}</p>
      ) : null}
    </div>
  );
}

export interface DependencyChecklistProps {
  group: DependencyGroup;
  title?: string;
  description?: string;
  className?: string;
}

export function DependencyChecklist({
  group,
  title = "Requirements",
  description,
  className,
}: DependencyChecklistProps) {
  const dependencies = useDependencies(group);
  const remediate = useRunDependencyRemediation();

  return (
    <section
      className={cn("rounded-lg border border-border p-4", className)}
      aria-label={title}
    >
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="text-sm font-medium">{title}</p>
          {description !== undefined ? (
            <p className="mt-1 text-xs text-muted-foreground">{description}</p>
          ) : null}
        </div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={dependencies.isFetching}
          onClick={() => void dependencies.refetch()}
        >
          <RefreshCw
            className={dependencies.isFetching ? "animate-spin" : ""}
            aria-hidden="true"
          />
          Recheck
        </Button>
      </div>

      {dependencies.isPending ? (
        <div className="mt-3 h-20 animate-pulse rounded-lg bg-muted" />
      ) : dependencies.isError ? (
        <p className="mt-3 text-sm text-destructive">
          Could not check requirements — {toAppError(dependencies.error).message}
        </p>
      ) : (
        <ul className="mt-3 flex flex-col gap-3">
          {dependencies.data.map((dependency) => {
            const meta = STATE_META[dependency.state];
            const Icon = meta.icon;
            const pending =
              remediate.isPending && remediate.variables === dependency.id;
            return (
              <li key={dependency.id} className="flex gap-3">
                <Icon
                  className={cn("mt-0.5 size-4 shrink-0", meta.tone)}
                  aria-hidden="true"
                />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium">{dependency.label}</span>
                    <Badge variant={meta.badge}>{meta.label}</Badge>
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {dependency.description}
                  </p>
                  {dependency.detail !== null ? (
                    <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                      {dependency.detail}
                    </p>
                  ) : null}
                  <RemediationAction
                    dependency={dependency}
                    pending={pending}
                    onRun={(id) => remediate.mutate(id)}
                  />
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
