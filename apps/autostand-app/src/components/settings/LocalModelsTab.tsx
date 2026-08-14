import {
  Check,
  Download,
  Gauge,
  MemoryStick,
  Pause,
  ShieldCheck,
  Trash2,
  Zap,
} from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import { Progress } from "@autostand/ui/components/progress";

import { DependencyChecklist } from "@/components/settings/DependencyChecklist";
import {
  DEPENDENCY_IDS,
  findDependency,
  useDependencies,
} from "@/hooks/use-dependencies";
import {
  useAcceptLocalModelTerms,
  useCancelLocalModelDownload,
  useDeleteLocalModel,
  useDownloadLocalModel,
  useLocalModels,
  useSelectLocalModel,
  useUnloadLocalModels,
} from "@/hooks/use-local-models";
import { useConfig, useSetConfig } from "@/hooks/use-config";
import type {
  LocalModelInfo,
  LocalRuntimePolicy,
  LocalRuntimeUnload,
} from "@/lib/types";
import { cn } from "@/lib/utils";

function formatBytes(bytes: number): string {
  return `${(bytes / 1024 ** 3).toFixed(2)} GiB`;
}

/** Caches are megabyte-scale, so the GiB-only model formatter would read 0.00. */
function formatFreed(bytes: number): string {
  if (bytes < 1024 ** 2) return `${Math.round(bytes / 1024)} KiB`;
  if (bytes < 1024 ** 3) return `${Math.round(bytes / 1024 ** 2)} MiB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GiB`;
}

/** Report the two effects separately; claiming neither happened is the honest
 * outcome when the runtime was already cold. */
function describeUnload(result: LocalRuntimeUnload): string {
  const freed: string[] = [];
  if (result.processes_terminated > 0) {
    freed.push(
      `${result.processes_terminated} runtime ${
        result.processes_terminated === 1 ? "process" : "processes"
      } stopped`,
    );
  }
  if (result.caches_removed > 0) {
    freed.push(`${formatFreed(result.bytes_freed)} of cached state deleted`);
  }
  return freed.length === 0
    ? "Nothing was loaded: no runtime process was running and no cache existed."
    : `Freed ${freed.join(" and ")}.`;
}

function statusVariant(model: LocalModelInfo) {
  if (model.status === "available") return "success" as const;
  if (model.status === "downloading") return "warning" as const;
  if (model.status === "error" || model.status === "corrupted") {
    return "error" as const;
  }
  return "secondary" as const;
}

export function LocalModelsTab() {
  const config = useConfig();
  const setConfig = useSetConfig();
  const models = useLocalModels();
  const download = useDownloadLocalModel();
  const cancel = useCancelLocalModelDownload();
  const remove = useDeleteLocalModel();
  const select = useSelectLocalModel();
  const acceptTerms = useAcceptLocalModelTerms();
  const unload = useUnloadLocalModels();
  const dependencies = useDependencies("local_ai");
  // Only a positive "missing" blocks selection. An unresolved or failed probe
  // must never disable a control that would otherwise have worked — and a
  // download stays available either way, since it is the prerequisite for
  // using a model once the runtime is installed.
  const runtimeMissing =
    findDependency(dependencies.data, DEPENDENCY_IDS.runtime)?.state ===
    "missing";
  const runtimePolicy = config.data?.llm.local_runtime_policy ?? "on_demand";
  const cachedBytes = (models.data ?? []).reduce(
    (total, model) => total + model.runtime_cache_bytes,
    0,
  );
  // An installed model is the precondition for a runtime process too: nothing
  // can be holding weights that are not on disk.
  const nothingToUnload =
    cachedBytes === 0 &&
    !(models.data ?? []).some((model) => model.status === "available");

  function setRuntimePolicy(local_runtime_policy: LocalRuntimePolicy) {
    if (config.data === undefined) return;
    setConfig.mutate({
      ...config.data,
      llm: { ...config.data.llm, local_runtime_policy },
    });
  }

  if (models.isPending) {
    return <p className="text-sm text-muted-foreground">Loading model catalog…</p>;
  }
  if (models.isError) {
    return <p className="text-sm text-destructive">Model catalog unavailable.</p>;
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="rounded-lg border border-border bg-muted/30 p-4">
        <p className="font-medium">Private, on-device fallback</p>
        <p className="text-sm text-muted-foreground">
          Models are never downloaded automatically. Files are resumed safely,
          verified with SHA-256 and stored in Autostand app data.
        </p>
      </div>

      <DependencyChecklist
        group="local_ai"
        description="On-device rendering needs all three. Downloading a model works without them."
      />

      <div className="flex flex-col gap-3">
        <div>
          <p className="font-medium">How should the local model run?</p>
          <p className="text-sm text-muted-foreground">
            This choice applies equally to Compile now and automatic scheduler runs.
          </p>
        </div>
        <div className="grid gap-3 md:grid-cols-2">
          {([
            {
              value: "keep_ready",
              title: "Keep a reusable cache",
              description:
                "Keep the model's reusable prompt state on this device so later runs can start faster. Uses additional disk space.",
              icon: Gauge,
            },
            {
              value: "on_demand",
              title: "Load on demand",
              description:
                "Start clean when a compile needs Local AI and remove reusable state afterwards. Slower, with the smallest footprint.",
              icon: Zap,
            },
          ] as const).map((option) => {
            const selected = runtimePolicy === option.value;
            const Icon = option.icon;
            return (
              <button
                key={option.value}
                type="button"
                aria-pressed={selected}
                disabled={config.data === undefined || setConfig.isPending}
                onClick={() => setRuntimePolicy(option.value)}
                className={cn(
                  "flex gap-3 rounded-lg border p-4 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  selected
                    ? "border-primary bg-primary/5"
                    : "border-border hover:bg-muted/50",
                )}
              >
                <Icon className="mt-0.5 size-4 shrink-0 text-primary" aria-hidden="true" />
                <span>
                  <span className="flex items-center gap-2 text-sm font-medium">
                    {option.title}
                    {selected ? <Badge variant="default">Selected</Badge> : null}
                  </span>
                  <span className="mt-1 block text-xs text-muted-foreground">
                    {option.description}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
      </div>

      <div className="flex flex-wrap items-start justify-between gap-4 rounded-lg border border-border p-4">
        <div>
          <p className="font-medium">Unload all models</p>
          <p className="text-sm text-muted-foreground">
            Stops any inference process still holding a model in memory and
            deletes the reusable prompt caches
            {cachedBytes > 0 ? ` (${formatFreed(cachedBytes)})` : ""}. Downloaded
            model files are kept.
          </p>
          {unload.data !== undefined && !unload.isPending ? (
            <p className="mt-2 text-xs text-muted-foreground">
              {describeUnload(unload.data)}
            </p>
          ) : null}
        </div>
        <Button
          type="button"
          variant="outline"
          disabled={nothingToUnload || unload.isPending}
          onClick={() => unload.mutate()}
        >
          <MemoryStick aria-hidden="true" />
          {unload.isPending ? "Unloading…" : "Unload all models"}
        </Button>
      </div>

      {models.data.map((model) => {
        const progress =
          model.size_bytes === 0
            ? 0
            : (model.downloaded_bytes / model.size_bytes) * 100;
        return (
          <div
            key={model.id}
            className="flex flex-col gap-4 rounded-lg border border-border p-4"
          >
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <div className="flex flex-wrap items-center gap-2">
                  <p className="font-medium">{model.display_name}</p>
                  <Badge variant="outline">{model.tier}</Badge>
                  <Badge variant={statusVariant(model)}>
                    {model.selected ? "Selected" : model.status.replaceAll("_", " ")}
                  </Badge>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                  {model.quality} · {model.format} · {formatBytes(model.size_bytes)} ·{" "}
                  {model.context_length.toLocaleString()} context
                </p>
                <a
                  href={model.license_url}
                  target="_blank"
                  rel="noreferrer"
                  className="text-xs text-primary underline-offset-4 hover:underline"
                >
                  {model.license} license
                </a>
              </div>

              <div className="flex flex-wrap justify-end gap-2">
                {model.status === "downloading" ? (
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => cancel.mutate(model.id)}
                  >
                    <Pause aria-hidden="true" /> Cancel
                  </Button>
                ) : null}
                {model.status === "not_downloaded" ||
                model.status === "error" ||
                model.status === "corrupted" ? (
                  <Button
                    type="button"
                    onClick={() => download.mutate(model.id)}
                  >
                    <Download aria-hidden="true" />
                    {model.downloaded_bytes > 0 ? "Resume" : "Download"}
                  </Button>
                ) : null}
                {model.status === "available" && !model.selected ? (
                  <Button
                    type="button"
                    disabled={runtimeMissing}
                    onClick={() => select.mutate(model.id)}
                  >
                    <Check aria-hidden="true" /> Use model
                  </Button>
                ) : null}
                {model.status === "available" ? (
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => remove.mutate(model.id)}
                  >
                    <Trash2 aria-hidden="true" /> Delete
                  </Button>
                ) : null}
              </div>
            </div>

            {runtimeMissing && model.status === "available" && !model.selected ? (
              <p className="text-xs text-warning">
                Install the llama.cpp runtime listed above before selecting this
                model. Downloading is unaffected.
              </p>
            ) : null}

            {model.status === "downloading" || model.downloaded_bytes > 0 ? (
              <div className="flex flex-col gap-1">
                <Progress value={Math.min(100, progress)} />
                <p className="text-xs text-muted-foreground">
                  {formatBytes(model.downloaded_bytes)} of {formatBytes(model.size_bytes)}
                </p>
              </div>
            ) : null}

            {model.terms_required ? (
              <div className="flex flex-wrap items-center justify-between gap-3 rounded-md bg-warning-bg p-3">
                <p className="text-xs text-warning">
                  Gemma requires acceptance of Google&apos;s model terms before download.
                </p>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => acceptTerms.mutate(model.id)}
                >
                  <ShieldCheck aria-hidden="true" /> Accept terms
                </Button>
              </div>
            ) : null}

            {model.error !== null ? (
              <p className="text-xs text-destructive">{model.error}</p>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
