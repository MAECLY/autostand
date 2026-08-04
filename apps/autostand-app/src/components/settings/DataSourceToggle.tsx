/**
 * One row of the Settings → Data Sources tab.
 *
 * `local-git` is pinned on: every AUTO bullet has to trace back to a commit,
 * so disabling it would leave the audit sidecar unable to verify anything.
 */

import { Switch } from "@autostand/ui/components/switch";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@autostand/ui/components/tooltip";

import { useToggleDataSource } from "@/hooks/use-data-sources";
import type { DataSourceConfig } from "@/lib/types";

/** Both spellings appear across the config (`local_git`) and the source list (`local-git`). */
const ALWAYS_ON_IDS: ReadonlySet<string> = new Set(["local-git", "local_git"]);

const ALWAYS_ON_REASON =
  "local-git is the authoritative source — every AUTO bullet must trace back to a commit, so it cannot be turned off.";

export interface DataSourceToggleProps {
  source: DataSourceConfig;
}

export function DataSourceToggle({ source }: DataSourceToggleProps) {
  const toggleDataSource = useToggleDataSource();
  const alwaysOn = ALWAYS_ON_IDS.has(source.id);

  const control = (
    <Switch
      checked={alwaysOn || source.enabled}
      disabled={alwaysOn || toggleDataSource.isPending}
      aria-label={`Enable ${source.label}`}
      onCheckedChange={(enabled) =>
        toggleDataSource.mutate({ id: source.id, enabled })
      }
    />
  );

  return (
    <div className="flex items-start justify-between gap-6 py-3">
      <div className="min-w-0">
        <p className="text-sm font-medium text-foreground">{source.label}</p>
        <p className="text-sm text-muted-foreground">{source.description}</p>
      </div>

      {alwaysOn ? (
        <TooltipProvider>
          <Tooltip>
            {/* A disabled Switch swallows pointer events, so the tooltip hangs off a wrapper. */}
            <TooltipTrigger asChild>
              <span tabIndex={0} className="rounded-md">
                {control}
              </span>
            </TooltipTrigger>
            <TooltipContent className="max-w-xs">
              {ALWAYS_ON_REASON}
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      ) : (
        control
      )}
    </div>
  );
}
