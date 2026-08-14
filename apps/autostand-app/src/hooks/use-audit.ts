/**
 * Audit sidecars: the per-render provenance JSON written next to every standup.
 *
 * `["audit", date]` lists the sidecars for a filing date (one per host);
 * `["audit-sidecar", path]` holds the parsed contents of one of them.
 */

import { useQuery } from "@tanstack/react-query";

import { tauriApi } from "@/lib/tauri";
import type { AuditSidecar } from "@/lib/types";

export function auditSidecarsKey(date: string) {
  return ["audit", date] as const;
}

export function auditSidecarKey(path: string) {
  return ["audit-sidecar", path] as const;
}

export function useAuditSidecars<TData = AuditSidecar[]>(
  date: string,
  select?: (sidecars: AuditSidecar[]) => TData,
) {
  return useQuery({
    queryKey: auditSidecarsKey(date),
    queryFn: () => tauriApi.listAuditSidecars(date),
    enabled: date.length > 0,
    select,
  });
}

export interface RenderProvenance {
  /** Provider id that rendered this host's block (`claude`, `ollama`, …). */
  provider: string | null;
  model: string | null;
  /** The preferred provider failed and another one took over. */
  fellback: boolean;
  renderUsed: AuditSidecar["render_used"];
}

/** Project one host's sidecar onto its render provenance; `null` when absent. */
export function selectRenderProvenance(
  sidecars: AuditSidecar[],
  hostSlug: string,
): RenderProvenance | null {
  const mine = sidecars.find((sidecar) => sidecar.host === hostSlug);
  if (!mine) return null;

  return {
    provider: mine.provider,
    model: mine.model,
    fellback: mine.fellback,
    renderUsed: mine.render_used,
  };
}

/**
 * Which provider and model rendered `hostSlug`'s block on `date`.
 *
 * A `select` over the sidecar listing, so it shares that query's cache entry
 * instead of adding an IPC call: this is metadata *about* the standup, and it
 * must not cost anything the standup itself does not already pay for.
 */
export function useRenderProvenance(
  date: string,
  hostSlug: string | null | undefined,
) {
  return useAuditSidecars(date, (sidecars) =>
    hostSlug ? selectRenderProvenance(sidecars, hostSlug) : null,
  );
}

export function useAuditSidecar(path: string | null | undefined) {
  return useQuery({
    // The empty-string fallback is unreachable: `enabled` gates the fetch, and
    // it keeps the key stable while no sidecar is selected.
    queryKey: auditSidecarKey(path ?? ""),
    queryFn: () => tauriApi.readAuditSidecar(path ?? ""),
    enabled: Boolean(path),
    // A sidecar is immutable once written — only a new render replaces it.
    staleTime: Infinity,
  });
}
