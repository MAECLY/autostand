/**
 * Whether the authoritative local-git source can gather anything at all.
 *
 * A query rather than a mutation: the answer is the reason a standup came back
 * empty, so it has to be on screen before the user asks for anything. The
 * backend probe is a depth-1 directory read plus one `git config` call, which is
 * cheap enough to run on mount.
 *
 * Nothing invalidates this automatically — every surface that writes
 * `github_dir`, `standup_authors` or `git_refs` invalidates
 * {@link standupReadinessKey} itself, because those are the only three inputs.
 */

import { useQuery } from "@tanstack/react-query";

import { tauriApi } from "@/lib/tauri";

export const standupReadinessKey = ["standup-readiness"] as const;

export function useStandupReadiness() {
  return useQuery({
    queryKey: standupReadinessKey,
    queryFn: tauriApi.getStandupReadiness,
  });
}
