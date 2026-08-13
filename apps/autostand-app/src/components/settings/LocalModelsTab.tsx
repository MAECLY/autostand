import { Check, Download, Pause, ShieldCheck, Trash2 } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import { Progress } from "@autostand/ui/components/progress";

import {
  useAcceptLocalModelTerms,
  useCancelLocalModelDownload,
  useDeleteLocalModel,
  useDownloadLocalModel,
  useLocalModels,
  useSelectLocalModel,
} from "@/hooks/use-local-models";
import type { LocalModelInfo } from "@/lib/types";

function formatBytes(bytes: number): string {
  return `${(bytes / 1024 ** 3).toFixed(2)} GiB`;
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
  const models = useLocalModels();
  const download = useDownloadLocalModel();
  const cancel = useCancelLocalModelDownload();
  const remove = useDeleteLocalModel();
  const select = useSelectLocalModel();
  const acceptTerms = useAcceptLocalModelTerms();

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
                  <Button type="button" onClick={() => select.mutate(model.id)}>
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
