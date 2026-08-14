/**
 * Settings → Providers: the quota ledger in the right-hand rail.
 *
 * Reading rules, in order of importance:
 *
 * 1. Every number here is provider-reported. A field the provider did not send
 *    renders as "No data", never as `0%` — a fabricated zero sends someone
 *    hunting for a quota problem that does not exist (`AGENTS.md` § Invariants).
 * 2. Units are formatted at this edge. Percentages are one unit among dollars,
 *    credits, requests and tokens; nothing here assumes a percent.
 * 3. A `balance` drains and a `consumption` window fills, so only the latter
 *    gets a bar. A balance has no denominator to draw against.
 */

import { Fragment } from "react";
import { RefreshCw } from "lucide-react";
import { formatDistanceToNowStrict, isValid, parseISO } from "date-fns";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import { Progress } from "@autostand/ui/components/progress";

import {
  useProviderHealth,
  useRefreshProviderHealth,
} from "@/hooks/use-providers";
import { periodLabel, remainingPercent, windowLabel } from "@/lib/usage";
import { cn } from "@/lib/utils";
import type {
  Pace,
  ProviderAvailability,
  ProviderHealth,
  UsageUnit,
  UsageWindow,
} from "@/lib/types";

/** The one string that stands in for an unreported value. */
const NO_DATA = "No data";

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

const PACE_LABELS: Record<Pace, string> = {
  ahead: "Ahead of pace",
  on_track: "On track",
  behind: "Runs out before reset",
};

/** Pace is a projection, not a failure — `behind` warns, it never alarms. */
const PACE_INDICATORS: Record<Pace, string> = {
  ahead: "bg-success",
  on_track: "bg-primary",
  behind: "bg-warning",
};

/** Noun appended after the amount. Empty where the format already carries it. */
const UNIT_NOUNS: Record<UsageUnit, string> = {
  percent: "",
  usd: "",
  credits: "credits",
  requests: "requests",
  tokens: "tokens",
  count: "",
};

const decimalFormat = new Intl.NumberFormat("en-US", {
  maximumFractionDigits: 2,
});
const usdFormat = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 2,
});

/** Render a raw scalar in the unit the provider reported it in. */
function formatAmount(
  value: number,
  unit: UsageUnit | null | undefined,
): string {
  if (!Number.isFinite(value)) return NO_DATA;
  if (unit === "percent") return `${Math.round(value)}%`;
  if (unit === "usd") return usdFormat.format(value);
  const amount = decimalFormat.format(value);
  const noun = unit === null || unit === undefined ? "" : UNIT_NOUNS[unit];
  return noun.length > 0 ? `${amount} ${noun}` : amount;
}

/**
 * The headline figure for a window.
 *
 * A balance reads as what is left; a consumption window reads as used-of-limit
 * when the provider sent both raw scalars, and otherwise falls back to whichever
 * percentage it did send. Nothing reported means nothing claimed.
 */
function primaryFigure(window: UsageWindow): string {
  const unit = window.unit;

  if (window.kind === "balance") {
    if (typeof window.available === "number") {
      return `${formatAmount(window.available, unit)} left`;
    }
    if (window.remaining_percent !== null) {
      return `${Math.round(window.remaining_percent)}% left`;
    }
    return NO_DATA;
  }

  if (
    typeof window.used === "number" &&
    typeof window.limit === "number" &&
    unit !== "percent"
  ) {
    return `${formatAmount(window.used, unit)} of ${formatAmount(window.limit, unit)}`;
  }
  if (window.remaining_percent !== null) {
    return `${Math.round(window.remaining_percent)}% left`;
  }
  if (window.used_percent !== null) {
    return `${Math.round(window.used_percent)}% used`;
  }
  return NO_DATA;
}

interface TimeNote {
  text: string;
  /** Absolute instant, parked in `title` so the exact time is one hover away. */
  title: string;
}

/** Relative time with the tense picked from which side of now the instant is. */
function timeNote(
  iso: string | null,
  future: string,
  past: string,
): TimeNote | null {
  if (iso === null) return null;
  const parsed = parseISO(iso);
  if (!isValid(parsed)) return null;
  const distance = formatDistanceToNowStrict(parsed);
  return {
    text:
      parsed.getTime() > Date.now()
        ? `${future} in ${distance}`
        : `${past} ${distance} ago`,
    title: parsed.toLocaleString(),
  };
}

interface MetaNote {
  key: string;
  text: string;
  title?: string;
}

/** Dot-separated footnotes, so a row's context stays on one line. */
function MetaLine({ notes }: { notes: MetaNote[] }) {
  if (notes.length === 0) return null;
  return (
    <p className="flex flex-wrap items-center gap-x-1.5 text-[11px] text-muted-foreground">
      {notes.map((note, index) => (
        <Fragment key={note.key}>
          {index > 0 ? <span aria-hidden="true">·</span> : null}
          <span title={note.title}>{note.text}</span>
        </Fragment>
      ))}
    </p>
  );
}

function UsageWindowRow({ window }: { window: UsageWindow }) {
  const label = windowLabel(window);
  const figure = primaryFigure(window);
  const remaining = remainingPercent(window);
  const pace = window.pace ?? null;

  const notes: MetaNote[] = [];
  const period = periodLabel(window.period_duration_ms);
  if (period !== null) notes.push({ key: "period", text: period });
  const reset = timeNote(window.resets_at, "Resets", "Reset");
  if (reset !== null) {
    notes.push({ key: "reset", text: reset.text, title: reset.title });
  }
  if (pace !== null) notes.push({ key: "pace", text: PACE_LABELS[pace] });

  return (
    <div className="flex flex-col gap-1">
      {/* Label left, figure right: one column of monospace tabular numerals runs
          down the whole rail, so quantities compare at a glance. */}
      <div className="flex items-baseline justify-between gap-2 text-xs">
        <span className="min-w-0 truncate">{label}</span>
        <span
          className={cn(
            "shrink-0 font-mono tabular-nums",
            figure === NO_DATA ? "text-muted-foreground" : "text-foreground",
          )}
        >
          {figure}
        </span>
      </div>
      {remaining !== null ? (
        <Progress
          value={remaining}
          className="h-1.5"
          aria-label={`${label} remaining`}
          indicatorClassName={pace !== null ? PACE_INDICATORS[pace] : undefined}
        />
      ) : null}
      <MetaLine notes={notes} />
    </div>
  );
}

function ProviderHealthRow({
  health,
  compact,
  onRefresh,
  pending,
  refreshing,
}: {
  health: ProviderHealth;
  compact: boolean;
  onRefresh: (provider: string) => void;
  /** Any refresh is in flight — every trigger locks, only the target spins. */
  pending: boolean;
  refreshing: boolean;
}) {
  const plan = health.plan ?? "";
  const notice = health.notice ?? "";
  const checked = timeNote(health.checked_at, "Checked", "Checked");

  const footnotes: MetaNote[] = [];
  if (health.source !== "unknown") {
    footnotes.push({
      key: "source",
      text: `Source: ${health.source.replaceAll("_", " ")}`,
    });
  }
  if (checked !== null) {
    footnotes.push({ key: "checked", text: checked.text, title: checked.title });
  }

  return (
    <section
      className={cn(
        "flex flex-col border-b border-border last:border-b-0",
        compact ? "gap-2 py-2.5" : "gap-2.5 py-3.5",
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex min-w-0 flex-col gap-1">
          {/* Identity and verdict on one wrapping line: a plan turns a bare
              percentage into a quantity ("82% of Max 20x" means something). */}
          <div className="flex min-w-0 flex-wrap items-center gap-1.5">
            <span className="truncate text-sm font-medium capitalize">
              {health.provider}
            </span>
            {plan.length > 0 ? <Badge variant="secondary">{plan}</Badge> : null}
            <Badge variant={availabilityVariant(health.availability)}>
              {AVAILABILITY_LABELS[health.availability]}
            </Badge>
            {health.stale === true ? (
              <Badge
                variant="outline"
                className="text-muted-foreground"
                title="Last good reading. The newest check did not land."
              >
                Cached
              </Badge>
            ) : null}
          </div>
          {notice.length > 0 ? (
            <p className="text-[11px] text-warning">{notice}</p>
          ) : null}
        </div>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-7 shrink-0"
          disabled={pending}
          aria-label={`Refresh ${health.provider} usage`}
          onClick={() => onRefresh(health.provider)}
        >
          <RefreshCw
            className={refreshing ? "animate-spin" : undefined}
            aria-hidden="true"
          />
        </Button>
      </div>

      {/* The rail: a hairline binds every window to the provider above it, so
          the panel reads as blocks rather than a stack of loose rows. */}
      {health.windows.length > 0 ? (
        <div className="flex flex-col gap-2 border-l border-border pl-3">
          {health.windows.map((window) => (
            <UsageWindowRow key={window.id} window={window} />
          ))}
        </div>
      ) : null}

      {/* The reason used to render only when `compact` was false, and the sole
          call site passes `compact` — so the explanation behind "Exhausted" or
          "Sign-in required" was never actually visible to anyone. */}
      {health.reason !== null ? (
        <p className="text-xs text-muted-foreground">
          {reasonLabel(health.reason)}
        </p>
      ) : null}

      <MetaLine notes={footnotes} />
    </section>
  );
}

/**
 * `compact` tightens the vertical rhythm for the sticky rail. Both densities
 * carry the same information — a narrower column is never a reason to drop a
 * number the provider actually reported.
 */
export function ProviderUsage({ compact = false }: { compact?: boolean }) {
  const health = useProviderHealth();
  const refresh = useRefreshProviderHealth();
  // `variables` is the provider the in-flight refresh targets; `undefined` is
  // the all-providers pass. Without it every row's icon spun at once.
  const target = refresh.isPending ? refresh.variables : undefined;
  const rows = health.data ?? [];

  return (
    <div className="flex flex-col">
      <div className="flex items-start justify-between gap-2 pb-2">
        <div className="min-w-0">
          <h3 className="text-sm font-semibold">Usage &amp; availability</h3>
          <p className="text-xs text-muted-foreground">
            Provider-reported values only.
          </p>
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-7 shrink-0 px-2 text-xs"
          disabled={refresh.isPending}
          onClick={() => refresh.mutate(undefined)}
        >
          <RefreshCw
            className={
              refresh.isPending && target === undefined
                ? "animate-spin"
                : undefined
            }
            aria-hidden="true"
          />
          Refresh all
        </Button>
      </div>

      {health.isPending ? (
        <p className="py-4 text-sm text-muted-foreground">Loading usage…</p>
      ) : null}
      {health.isError ? (
        <p className="py-4 text-sm text-destructive">
          Provider usage could not be loaded.
        </p>
      ) : null}
      {!health.isPending && !health.isError && rows.length === 0 ? (
        <p className="py-4 text-sm text-muted-foreground">
          No provider is configured yet. Add one on the left to track its quota.
        </p>
      ) : null}

      {rows.length > 0 ? (
        <div className="flex flex-col border-t border-border">
          {rows.map((item) => (
            <ProviderHealthRow
              key={item.provider}
              health={item}
              compact={compact}
              pending={refresh.isPending}
              refreshing={target === item.provider}
              onRefresh={(provider) => refresh.mutate(provider)}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}
