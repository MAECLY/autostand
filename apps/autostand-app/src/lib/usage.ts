/**
 * Reading a quota snapshot the same way everywhere.
 *
 * The status-bar badge, the pre-flight before a compile and the Settings rail
 * all answer the same question — *how close is this provider to running out?* —
 * so they share one implementation of it. Three rules hold throughout:
 *
 * 1. **Never invent.** A field the provider did not send stays `null`. `0` is a
 *    claim; absence is not.
 * 2. **One threshold.** "Low" is whatever `notifications.low_usage_threshold_percent`
 *    says it is. Nothing here hardcodes a percentage.
 * 3. **The projection comes from the backend.** `runs_out_in_seconds` is
 *    computed by `autostand_core::pace`, under the same minimum-elapsed guard as
 *    `pace`. When the backend declined to project, so does the UI.
 */

import type { AppConfig, ProviderHealth, UsageWindow } from "@/lib/types";

/**
 * The provider a compile reaches for first.
 *
 * Mirrors `render::provider_chain`: an explicit `provider_order` wins, and
 * `preferred_provider` is the legacy fallback. Reading it any other way would
 * let the badge name one provider while the render used another.
 */
export function activeProvider(config: AppConfig | undefined): string | null {
  if (config === undefined) return null;
  const ordered = config.llm.provider_order
    .map((id) => id.trim())
    .filter((id) => id.length > 0);
  const active = ordered[0] ?? config.llm.preferred_provider.trim();
  return active.length > 0 ? active : null;
}

/**
 * Availability values the render chain skips outright.
 *
 * Kept in step with `render::health_skip_reason`: offering a provider the
 * backend is about to pass over would be a dead end.
 */
const SKIPPED_AVAILABILITY: readonly ProviderHealth["availability"][] = [
  "exhausted",
  "rate_limited",
  "auth_required",
];

/**
 * The provider the pre-flight offers instead, or `null` when there is no
 * credible alternative.
 *
 * Only a configured, enabled provider that is *not* itself under pressure is
 * offered: suggesting a swap onto a second exhausted provider would be noise.
 * A provider with no snapshot at all is a fair suggestion — unknown is not bad
 * news, it is no news.
 */
export function fallbackProvider(
  config: AppConfig | undefined,
  health: ProviderHealth[] | undefined,
  thresholdPercent: number,
): string | null {
  if (config === undefined || !config.llm.fallback_enabled) return null;
  const active = activeProvider(config);
  const ordered = config.llm.provider_order
    .map((id) => id.trim())
    .filter((id) => id.length > 0);
  const candidates =
    ordered.length > 0
      ? ordered
      : config.llm.providers.filter((entry) => entry.enabled).map((entry) => entry.id);
  for (const candidate of candidates) {
    if (candidate === active) continue;
    const snapshot = healthFor(health, candidate);
    if (snapshot !== undefined && SKIPPED_AVAILABILITY.includes(snapshot.availability)) {
      continue;
    }
    if (usagePressure(snapshot, thresholdPercent) !== null) continue;
    return candidate;
  }
  return null;
}

/**
 * Remaining share of a consumption window, 0–100, or `null` when unknowable.
 *
 * Deriving the share from `used`/`limit` is arithmetic over two reported values,
 * not an invented reading — but a balance never gets one, because a countdown
 * has no denominator to fill.
 */
export function remainingPercent(window: UsageWindow): number | null {
  if (window.kind === "balance") return null;
  if (window.remaining_percent !== null && window.remaining_percent !== undefined) {
    return clampPercent(window.remaining_percent);
  }
  if (window.used_percent !== null && window.used_percent !== undefined) {
    return clampPercent(100 - window.used_percent);
  }
  if (
    typeof window.used === "number" &&
    typeof window.limit === "number" &&
    window.limit > 0
  ) {
    return clampPercent(100 - (window.used / window.limit) * 100);
  }
  return null;
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(100, Math.max(0, value));
}

export interface TightestWindow {
  window: UsageWindow;
  /** Share left of this window, 0–100. */
  remaining: number;
}

/**
 * The window closest to running out, or `null` when none reports a share.
 *
 * Mirrors `ProviderSnapshot::tightest_remaining_percent` on the Rust side: a
 * window without a percentage is passed over, never counted as empty.
 */
export function tightestWindow(
  health: ProviderHealth | null | undefined,
): TightestWindow | null {
  if (!health) return null;
  let tightest: TightestWindow | null = null;
  for (const window of health.windows) {
    const remaining = remainingPercent(window);
    if (remaining === null) continue;
    if (tightest === null || remaining < tightest.remaining) {
      tightest = { window, remaining };
    }
  }
  return tightest;
}

/** The snapshot for one provider, or `undefined` when it has none. */
export function healthFor(
  health: ProviderHealth[] | undefined,
  provider: string,
): ProviderHealth | undefined {
  return health?.find((entry) => entry.provider === provider);
}

function trimZero(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

/** How long the quota is measured over, e.g. "5 h window". */
export function periodLabel(ms: number | null | undefined): string | null {
  if (typeof ms !== "number" || !Number.isFinite(ms) || ms <= 0) return null;
  const minutes = ms / 60_000;
  if (minutes < 60) return `${trimZero(minutes)} min window`;
  const hours = minutes / 60;
  if (hours < 48) return `${trimZero(hours)} h window`;
  return `${trimZero(hours / 24)} d window`;
}

/** The provider's own name for the window wins over the derived one. */
export function windowLabel(window: UsageWindow): string {
  const label = window.label ?? "";
  if (label.trim().length > 0) return label;
  return window.id
    .replaceAll("_", " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

/**
 * The window named as the user would say it: "the 5 h window", else its label.
 *
 * The period is preferred because a duration is what makes a percentage mean
 * something — "12% of the 5 h window" is actionable, "12% of Session" is not.
 */
export function windowDescription(window: UsageWindow): string {
  return periodLabel(window.period_duration_ms) ?? windowLabel(window);
}

/**
 * The backend's run-out projection as coarse English, or `null`.
 *
 * Deliberately rounded: a projection extrapolated from a burn rate is not a
 * clock, and "~35 min" is honest where "34 min 12 s" is not. `null` whenever the
 * backend declined to project — the sentence is then omitted, never guessed.
 */
export function formatRunOut(seconds: number | null | undefined): string | null {
  if (typeof seconds !== "number" || !Number.isFinite(seconds) || seconds <= 0) {
    return null;
  }
  const minutes = seconds / 60;
  if (minutes < 1) return "~1 min";
  if (minutes < 90) return `~${Math.round(minutes)} min`;
  const hours = minutes / 60;
  if (hours < 48) return `~${Math.round(hours)} h`;
  return `~${Math.round(hours / 24)} d`;
}

export interface UsagePressure {
  provider: string;
  window: UsageWindow;
  /** Share left of the tightest window, 0–100, already rounded for display. */
  remainingPercent: number;
  /** "5 h window" or the window's own label. */
  windowDescription: string;
  /** "~35 min", or `null` when the backend declined to project. */
  runsOutIn: string | null;
}

/**
 * The pressure a provider is under, or `null` when there is nothing to warn about.
 *
 * `null` covers three different situations on purpose, because the caller treats
 * them identically: no snapshot, no window with a readable share, and a share
 * that is comfortably above the configured threshold. Silence beats a badge that
 * says "unknown".
 */
export function usagePressure(
  health: ProviderHealth | null | undefined,
  thresholdPercent: number,
): UsagePressure | null {
  if (!health) return null;
  const tightest = tightestWindow(health);
  if (tightest === null) return null;
  if (tightest.remaining > thresholdPercent) return null;
  return {
    provider: health.provider,
    window: tightest.window,
    remainingPercent: Math.round(tightest.remaining),
    windowDescription: windowDescription(tightest.window),
    runsOutIn: formatRunOut(tightest.window.runs_out_in_seconds),
  };
}
