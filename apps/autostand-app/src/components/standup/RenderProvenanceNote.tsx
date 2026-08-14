/**
 * Informative footnote: which provider and model rendered this host's block.
 *
 * The provenance lives in the audit sidecar, never in the standup itself — a
 * teammate reading the markdown must not find out what wrote it. `pipeline_e2e.rs`
 * (`the_standup_file_never_names_the_render_provider`) guards the other end of
 * that invariant; this component is why the information is still reachable.
 */

import type { ReactNode } from "react";
import { Cpu, Sparkles } from "lucide-react";

import { useRenderProvenance } from "@/hooks/use-audit";
import { useLlmProviders } from "@/hooks/use-providers";

export interface ProviderLabelProps {
  /** Provider id as the sidecar recorded it (`claude`, `ollama`, …). */
  providerId: string;
}

/**
 * The provider's human label, falling back to its id.
 *
 * Its own component so `list_llm_providers` — which probes every provider CLI —
 * is only ever called for a standup that actually named a provider.
 */
function ProviderLabel({ providerId }: ProviderLabelProps) {
  const providers = useLlmProviders();
  const label = providers.data?.find(
    (provider) => provider.id === providerId,
  )?.label;

  return (
    <span className="font-medium text-foreground">{label ?? providerId}</span>
  );
}

interface ProvenanceLineProps {
  icon: ReactNode;
  children: ReactNode;
}

/** One muted line, deliberately quieter than anything in the preview above it. */
function ProvenanceLine({ icon, children }: ProvenanceLineProps) {
  return (
    <section
      aria-label="Render provenance"
      className="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs text-muted-foreground"
    >
      {icon}
      <span>{children}</span>
      <span className="text-muted-foreground/70">
        Shown here only; never written to the standup file.
      </span>
    </section>
  );
}

/** Why a fallback ended up deterministic: a rejected draft, or no provider at all. */
function FallbackReason({ provider }: { provider: string | null }) {
  if (!provider) return <>{" — no AI provider was available."}</>;

  return (
    <>
      {" — the draft from "}
      <ProviderLabel providerId={provider} />
      {" was not used."}
    </>
  );
}

export interface RenderProvenanceNoteProps {
  /** Filing date of the standup on screen (`YYYY-MM-DD`). */
  date: string;
  /** Slug of this machine — the block whose provenance is reported. */
  hostSlug?: string;
}

export function RenderProvenanceNote({
  date,
  hostSlug,
}: RenderProvenanceNoteProps) {
  const { data: provenance } = useRenderProvenance(date, hostSlug);

  // No sidecar for this host yet: the standup was filed by hand or by another
  // machine, and inventing a renderer for it would be a lie.
  if (!provenance) return null;

  const { provider, model, renderUsed, fellback } = provenance;
  // Either field alone is enough: `fellback` is what the pipeline decided,
  // `render_used` is what it wrote, and an older sidecar may carry only one.
  const fellBackToDeterministic = fellback || renderUsed === "llm_fallback";

  if (renderUsed === "llm" && provider && !fellBackToDeterministic) {
    return (
      <ProvenanceLine
        icon={<Sparkles className="size-3.5 shrink-0" aria-hidden="true" />}
      >
        Rendered by <ProviderLabel providerId={provider} />
        {model ? (
          <>
            {" · "}
            <span className="font-mono">{model}</span>
          </>
        ) : null}
      </ProvenanceLine>
    );
  }

  return (
    <ProvenanceLine
      icon={<Cpu className="size-3.5 shrink-0" aria-hidden="true" />}
    >
      Deterministic render
      {fellBackToDeterministic ? (
        <FallbackReason provider={provider} />
      ) : (
        " — no AI provider was used."
      )}
    </ProvenanceLine>
  );
}
