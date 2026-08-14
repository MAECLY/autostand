/** Dashboard Compile Now controls with safe regeneration review. */

import { useState } from "react";
import { Play } from "lucide-react";

import { Button } from "@autostand/ui/components/button";
import { Spinner } from "@autostand/ui/components/spinner";
import { Switch } from "@autostand/ui/components/switch";

import { QuotaPreflightDialog } from "@/components/standup/QuotaPreflightDialog";
import { RegenerationDialog } from "@/components/standup/RegenerationDialog";
import { useConfig, useSetConfig } from "@/hooks/use-config";
import { usePipelineStatus } from "@/hooks/use-pipeline-status";
import { useProviderHealth } from "@/hooks/use-providers";
import { useApplyRegeneration, usePreviewRegeneration } from "@/hooks/use-regeneration";
import type { AppConfig, PipelineState, RegenerationPreview } from "@/lib/types";
import {
  activeProvider,
  fallbackProvider,
  healthFor,
  usagePressure,
} from "@/lib/usage";

const BUSY_STATES: readonly PipelineState[] = ["gathering", "rendering"];

export interface CompileButtonProps {
  className?: string;
  date: string;
}

export function CompileButton({ className, date }: CompileButtonProps) {
  const config = useConfig();
  const health = useProviderHealth();
  const pipeline = usePipelineStatus();
  const setConfig = useSetConfig();
  const preview = usePreviewRegeneration();
  const apply = useApplyRegeneration();
  const [comparison, setComparison] = useState<RegenerationPreview | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [preflightOpen, setPreflightOpen] = useState(false);
  const replaceImmediately = config.data?.regeneration.replace_immediately ?? false;
  const state: PipelineState = pipeline.data?.state ?? "idle";
  const busy = BUSY_STATES.includes(state) || preview.isPending || apply.isPending;

  const threshold = config.data?.notifications.low_usage_threshold_percent ?? 0;
  const provider = activeProvider(config.data);
  const pressure = usagePressure(
    provider === null ? undefined : healthFor(health.data, provider),
    threshold,
  );
  const alternative = fallbackProvider(config.data, health.data, threshold);

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

  /**
   * The pre-flight informs, it never blocks: a quota warning is advice, and the
   * user asking for a standup outranks it. When nothing is under pressure the
   * dialog is skipped entirely, so the common path stays a single click.
   */
  function startCompile() {
    if (pressure === null) {
      compile();
      return;
    }
    setPreflightOpen(true);
  }

  /**
   * Put `alternative` at the head of the chain, then compile with it.
   *
   * The compile waits for the save: the render reads the provider order from the
   * config store, so firing both at once would race and could still render on
   * the provider the user just declined.
   */
  function switchProviderAndCompile(alternativeProvider: string) {
    setPreflightOpen(false);
    if (config.data === undefined) return;
    setConfig.mutate(withProviderFirst(config.data, alternativeProvider), {
      onSuccess: () => compile(),
    });
  }

  return (
    <>
      <div className="flex flex-col items-end gap-1.5">
        <Button className={className} disabled={busy} onClick={startCompile}>
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
      <QuotaPreflightDialog
        pressure={pressure}
        alternative={alternative}
        open={preflightOpen}
        onOpenChange={setPreflightOpen}
        onProceed={() => {
          setPreflightOpen(false);
          compile();
        }}
        onSwitch={switchProviderAndCompile}
      />
      <RegenerationDialog
        preview={comparison}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
      />
    </>
  );
}

/**
 * `config` with `provider` at the head of the failover chain.
 *
 * The order is otherwise preserved: switching provider for one crowded window is
 * not a reason to forget the rest of the user's preference. `preferred_provider`
 * moves too, because a legacy config with an empty `provider_order` reads that
 * field instead.
 */
function withProviderFirst(config: AppConfig, provider: string): AppConfig {
  const rest = config.llm.provider_order.filter((id) => id.trim() !== provider);
  return {
    ...config,
    llm: {
      ...config.llm,
      preferred_provider: provider,
      provider_order: [provider, ...rest],
    },
  };
}
