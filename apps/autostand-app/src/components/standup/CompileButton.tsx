/** Dashboard Compile Now controls with safe regeneration review. */

import { useState } from "react";
import { Play } from "lucide-react";

import { Button } from "@autostand/ui/components/button";
import { Spinner } from "@autostand/ui/components/spinner";
import { Switch } from "@autostand/ui/components/switch";

import { RegenerationDialog } from "@/components/standup/RegenerationDialog";
import { useConfig, useSetConfig } from "@/hooks/use-config";
import { usePipelineStatus } from "@/hooks/use-pipeline-status";
import { useApplyRegeneration, usePreviewRegeneration } from "@/hooks/use-regeneration";
import type { PipelineState, RegenerationPreview } from "@/lib/types";

const BUSY_STATES: readonly PipelineState[] = ["gathering", "rendering"];

export interface CompileButtonProps {
  className?: string;
  date: string;
}

export function CompileButton({ className, date }: CompileButtonProps) {
  const config = useConfig();
  const pipeline = usePipelineStatus();
  const setConfig = useSetConfig();
  const preview = usePreviewRegeneration();
  const apply = useApplyRegeneration();
  const [comparison, setComparison] = useState<RegenerationPreview | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const replaceImmediately = config.data?.regeneration.replace_immediately ?? false;
  const state: PipelineState = pipeline.data?.state ?? "idle";
  const busy = BUSY_STATES.includes(state) || preview.isPending || apply.isPending;

  function setReplaceImmediately(enabled: boolean) {
    if (config.data === undefined) return;
    setConfig.mutate({
      ...config.data,
      regeneration: { replace_immediately: enabled },
    });
  }

  function compile() {
    preview.mutate(date, {
      onSuccess: (candidate) => {
        if (replaceImmediately) {
          apply.mutate({ token: candidate.token, resolution: "use_candidate" });
        } else {
          setComparison(candidate);
          setDialogOpen(true);
        }
      },
    });
  }

  return (
    <>
      <div className="flex flex-col items-end gap-1.5">
        <Button className={className} disabled={busy} onClick={compile}>
          {busy ? (
            <>
              <Spinner size="sm" label="Compiling" />
              <span>{preview.isPending ? "Generating comparison…" : "Applying…"}</span>
            </>
          ) : (
            <><Play /> Compile now</>
          )}
        </Button>
        <label className="flex cursor-pointer items-center gap-2 text-xs text-muted-foreground">
          <Switch
            checked={replaceImmediately}
            disabled={config.data === undefined || setConfig.isPending || busy}
            aria-label="Replace standup immediately"
            onCheckedChange={setReplaceImmediately}
          />
          {replaceImmediately ? "Replace immediately" : "Review changes first"}
        </label>
      </div>
      <RegenerationDialog
        preview={comparison}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
      />
    </>
  );
}
