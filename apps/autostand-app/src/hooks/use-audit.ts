/**
 * Audit sidecars: the per-render provenance JSON written next to every standup.
 *
 * `["audit", date]` lists the sidecars for a filing date (one per host);
 * `["audit-sidecar", path]` holds the parsed contents of one of them.
 */

import { useQuery } from "@tanstack/react-query";

import { tauriApi } from "@/lib/tauri";

export function auditSidecarsKey(date: string) {
  return ["audit", date] as const;
}

export function auditSidecarKey(path: string) {
  return ["audit-sidecar", path] as const;
}

export function useAuditSidecars(date: string) {
  return useQuery({
    queryKey: auditSidecarsKey(date),
    queryFn: () => tauriApi.listAuditSidecars(date),
    enabled: date.length > 0,
  });
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
