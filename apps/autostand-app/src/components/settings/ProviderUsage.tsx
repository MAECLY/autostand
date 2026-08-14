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

/**
 * Backend reason codes are stable, lowercase and secret-free; the copy lives here.
 *
 * An unmapped code falls back to its own text with underscores spaced out, so a
 * new backend reason degrades to something readable rather than disappearing.
 */
const REASON_LABELS: Record<string, string> = {
  awaiting_first_refresh: "Not checked yet — refresh to read usage.",
  usage_not_programmatically_available:
    "This provider exposes usage only in its own interface.",
  usage_not_supported: "This provider has no usage contract.",
  usage_unavailable: "The provider did not return usage this time.",
  probe_failed: "The usage check could not complete.",
  cli_not_found: "The provider CLI was not found on PATH.",
  not_logged_in: "Sign in with the provider CLI, then refresh.",
  session_expired: "The saved sign-in expired — run the provider CLI once, then refresh.",
  // Named rather than folded into "unavailable": an API key can run renders but
  // cannot read subscription quota, and a generic message here sends the user
  // hunting for a problem that does not exist.
  usage_requires_cli_login:
    "An API key cannot read subscription usage — sign in with the provider CLI.",
  credential_store_unavailable:
    "The saved sign-in could not be read on this pass — refresh to try again.",
  rate_limited: "The provider is rate limiting usage checks — try again shortly.",
  network: "No connection to the provider.",
  timeout: "The usage check timed out.",
  unsupported_payload: "The provider's usage response changed shape.",
  unexpected_status: "The provider rejected the usage request.",
};

function reasonLabel(reason: string): string {
  return REASON_LABELS[reason] ?? reason.replaceAll("_", " ");
}

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

function ProviderHealthRow({
  health,
  compact = false,
  onRefresh,
  refreshing = false,
}: {
  health: ProviderHealth;
  compact?: boolean;
  onRefresh?: (provider: string) => void;
  refreshing?: boolean;
}) {
  return (
    <div
      className={
        compact
          ? "flex flex-col gap-2 border-b border-border py-3 last:border-b-0"
          : "flex flex-col gap-3 border-b border-border py-4 last:border-b-0"
      }
    >
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate font-medium capitalize">{health.provider}</p>
          {health.source !== "unknown" ? (
            <p className="text-xs text-muted-foreground">
              Source: {health.source.replaceAll("_", " ")}
            </p>
          ) : null}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Badge variant={availabilityVariant(health.availability)}>
            {AVAILABILITY_LABELS[health.availability]}
          </Badge>
          {onRefresh !== undefined ? (
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-7"
              disabled={refreshing}
              aria-label={`Refresh ${health.provider} usage`}
              onClick={() => onRefresh(health.provider)}
            >
              <RefreshCw
                className={refreshing ? "animate-spin" : undefined}
                aria-hidden="true"
              />
            </Button>
          ) : null}
        </div>
      </div>

      {health.windows.map((window) => (
        <UsageWindowRow key={window.id} window={window} />
      ))}

      {/* The reason used to render only when `compact` was false, and the sole
          call site passes `compact` — so the explanation behind "Exhausted" or
          "Sign-in required" was never actually visible to anyone. */}
      {health.reason !== null ? (
        <p className="text-xs text-muted-foreground">
          {reasonLabel(health.reason)}
        </p>
      ) : null}
    </div>
  );
}

export function ProviderUsage({ compact = false }: { compact?: boolean }) {
  const health = useProviderHealth();
  const refresh = useRefreshProviderHealth();

  return (
    <div className="flex flex-col">
      <div className="flex items-center justify-between gap-3">
        {compact ? (
          <div>
            <p className="text-sm font-semibold">Usage & availability</p>
            <p className="text-xs text-muted-foreground">Provider-reported values only.</p>
          </div>
        ) : <span />}
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
      {!health.isPending && !health.isError && health.data?.length === 0 ? (
        <p className="py-6 text-sm text-muted-foreground">
          No provider is configured yet.
        </p>
      ) : null}
      {health.data?.map((item) => (
        <ProviderHealthRow
          key={item.provider}
          health={item}
          compact={compact}
          refreshing={refresh.isPending}
          onRefresh={(provider) => refresh.mutate(provider)}
        />
      ))}
    </div>
  );
}
