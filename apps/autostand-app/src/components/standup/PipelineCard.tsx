/**
 * Live view of the compile pipeline: progress, current step, last run, and the
 * failure banner when the state machine lands in `error`.
 */

import { AlertTriangle } from "lucide-react";

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@autostand/ui/components/alert";
import { Badge } from "@autostand/ui/components/badge";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";
import { Progress } from "@autostand/ui/components/progress";

import { usePipelineStatus } from "@/hooks/use-pipeline-status";
import type { PipelineState } from "@/lib/types";
import { formatRelative } from "@/lib/utils";

const STATE_VARIANT: Record<
  PipelineState,
  "secondary" | "warning" | "success" | "error"
> = {
  idle: "secondary",
  gathering: "warning",
  rendering: "warning",
  done: "success",
  error: "error",
};

export interface PipelineCardProps {
  className?: string;
}

export function PipelineCard({ className }: PipelineCardProps) {
  const { data: status } = usePipelineStatus();

  const state: PipelineState = status?.state ?? "idle";
  const percent = status?.percent ?? 0;
  const step = status?.step;
  const lastRunAt = status?.last_run_at;

  return (
    <Card className={className}>
      <CardHeader className="flex flex-row items-center justify-between gap-2">
        <CardTitle className="text-sm font-medium text-subtle">
          Pipeline
        </CardTitle>
        <Badge variant={STATE_VARIANT[state]}>{state}</Badge>
      </CardHeader>
      <CardContent className="space-y-3">
        <Progress value={percent} />
        <div className="flex items-center justify-between text-sm">
          <span className="font-mono text-muted-foreground">
            {step ?? "no step"}
          </span>
          <span className="font-mono text-muted-foreground">{percent}%</span>
        </div>
        <p className="text-sm text-subtle">
          Last run {lastRunAt ? formatRelative(lastRunAt) : "never"}
        </p>
        {state === "error" && (
          <Alert variant="destructive">
            <AlertTriangle />
            <AlertTitle>Pipeline failed</AlertTitle>
            <AlertDescription>
              {status?.error ?? "The run ended without a diagnostic."}
            </AlertDescription>
          </Alert>
        )}
      </CardContent>
    </Card>
  );
}
