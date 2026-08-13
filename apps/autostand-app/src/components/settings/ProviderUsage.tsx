import { RefreshCw } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import { Progress } from "@autostand/ui/components/progress";

import {
  useProviderHealth,
  useRefreshProviderHealth,
} from "@/hooks/use-providers";
import type {
  ProviderAvailability,
  ProviderHealth,
  UsageWindow,
} from "@/lib/types";

const AVAILABILITY_LABELS: Record<ProviderAvailability, string> = {
  available: "Available",
  low: "Low usage",
  exhausted: "Exhausted",
  rate_limited: "Rate limited",
  auth_required: "Sign-in required",
  model_unavailable: "Model unavailable",
  unavailable: "Unavailable",
  unknown: "Usage unavailable",
};

function availabilityVariant(availability: ProviderAvailability) {
  if (availability === "available") return "success" as const;
  if (availability === "unknown") return "secondary" as const;
  if (availability === "low" || availability === "rate_limited") {
    return "warning" as const;
  }
  return "error" as const;
}

function windowLabel(window: UsageWindow): string {
  return window.id
    .replaceAll("_", " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function resetLabel(resetsAt: string | null): string | null {
  if (resetsAt === null) return null;
  const date = new Date(resetsAt);
  if (Number.isNaN(date.valueOf())) return null;
  return `Resets ${date.toLocaleString()}`;
}

function UsageWindowRow({ window }: { window: UsageWindow }) {
  const remaining = window.remaining_percent;
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between gap-3 text-xs">
        <span>{windowLabel(window)}</span>
        <span className="text-muted-foreground">
          {remaining === null ? "Unknown" : `${Math.round(remaining)}% remaining`}
        </span>
      </div>
      {remaining !== null ? <Progress value={remaining} /> : null}
      {resetLabel(window.resets_at) !== null ? (
        <p className="text-[11px] text-muted-foreground">
          {resetLabel(window.resets_at)}
        </p>
      ) : null}
    </div>
  );
}

function ProviderHealthRow({ health }: { health: ProviderHealth }) {
  return (
    <div className="flex flex-col gap-3 border-b border-border py-4 last:border-b-0">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="font-medium capitalize">{health.provider}</p>
          <p className="text-xs text-muted-foreground">
            {health.source === "unknown"
              ? "This provider does not expose a supported usage query."
              : `Source: ${health.source.replaceAll("_", " ")}`}
          </p>
        </div>
        <Badge variant={availabilityVariant(health.availability)}>
          {AVAILABILITY_LABELS[health.availability]}
        </Badge>
      </div>

      {health.windows.map((window) => (
        <UsageWindowRow key={window.id} window={window} />
      ))}

      {health.reason !== null ? (
        <p className="text-xs text-muted-foreground">{health.reason}</p>
      ) : null}
    </div>
  );
}

export function ProviderUsage() {
  const health = useProviderHealth();
  const refresh = useRefreshProviderHealth();

  return (
    <div className="flex flex-col">
      <div className="flex justify-end">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={refresh.isPending}
          onClick={() => refresh.mutate(undefined)}
        >
          <RefreshCw
            className={refresh.isPending ? "animate-spin" : undefined}
            aria-hidden="true"
          />
          Refresh usage
        </Button>
      </div>

      {health.isPending ? (
        <p className="py-6 text-sm text-muted-foreground">Loading usage…</p>
      ) : null}
      {health.isError ? (
        <p className="py-6 text-sm text-destructive">
          Provider usage could not be loaded.
        </p>
      ) : null}
      {health.data?.map((item) => (
        <ProviderHealthRow key={item.provider} health={item} />
      ))}
    </div>
  );
}
