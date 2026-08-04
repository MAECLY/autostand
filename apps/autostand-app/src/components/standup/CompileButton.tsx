/**
 * "Compile now" — the dashboard's single action. While the pipeline is live the
 * button reports the running step instead of its label.
 */

import { Play } from "lucide-react";

import { Button } from "@autostand/ui/components/button";
import { Spinner } from "@autostand/ui/components/spinner";

import { useTriggerRunNow } from "@/hooks/use-compile";
import { usePipelineStatus } from "@/hooks/use-pipeline-status";
import type { PipelineState } from "@/lib/types";

const BUSY_STATES: readonly PipelineState[] = ["gathering", "rendering"];

export interface CompileButtonProps {
  className?: string;
}

export function CompileButton({ className }: CompileButtonProps) {
  const { data: status } = usePipelineStatus();
  const triggerRunNow = useTriggerRunNow();

  const state: PipelineState = status?.state ?? "idle";
  // `isPending` covers the gap between the click and the first progress event.
  const busy = BUSY_STATES.includes(state) || triggerRunNow.isPending;
  const step = status?.step ?? "starting";
  const percent = status?.percent ?? 0;

  return (
    <Button
      className={className}
      disabled={busy}
      onClick={() => {
        triggerRunNow.mutate();
      }}
    >
      {busy ? (
        <>
          <Spinner size="sm" label="Compiling" />
          <span className="font-mono">
            {step} · {percent}%
          </span>
        </>
      ) : (
        <>
          <Play />
          Compile now
        </>
      )}
    </Button>
  );
}
