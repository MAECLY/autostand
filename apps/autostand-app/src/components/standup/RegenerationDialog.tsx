import { useEffect, useState } from "react";
import { ArrowLeft, ArrowRight, GitMerge } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@autostand/ui/components/dialog";
import { Textarea } from "@autostand/ui/components/textarea";

import { useApplyRegeneration } from "@/hooks/use-regeneration";
import type { RegenerationPreview } from "@/lib/types";

export function RegenerationDialog({
  preview,
  open,
  onOpenChange,
}: {
  preview: RegenerationPreview | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const apply = useApplyRegeneration();
  const [merged, setMerged] = useState("");

  useEffect(() => {
    if (preview !== null) setMerged(preview.candidate_auto);
  }, [preview]);

  if (preview === null) return null;

  // An arrow const, not a function declaration: a hoisted declaration escapes the
  // `preview === null` narrowing above, so `preview.token` would not type-check.
  const resolve = (
    resolution: "keep_current" | "use_candidate" | "merge",
    mergedAuto?: string,
  ) => {
    apply.mutate(
      { token: preview.token, resolution, mergedAuto },
      { onSuccess: () => onOpenChange(false) },
    );
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] max-w-6xl overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <GitMerge className="size-5" /> Review regenerated standup
          </DialogTitle>
          <DialogDescription>
            Nothing has been overwritten. Compare both AUTO blocks, keep one,
            or edit a combined result. Your MANUAL items are preserved.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-3 lg:grid-cols-2">
          <section className="min-w-0 rounded-lg border border-border">
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <p className="text-sm font-medium">Current</p>
              <Button type="button" size="sm" variant="ghost" onClick={() => setMerged(preview.current_auto)}>
                <ArrowLeft /> Copy to merge
              </Button>
            </div>
            <pre className="max-h-72 overflow-auto whitespace-pre-wrap p-3 font-mono text-xs">
              {preview.current_auto || "No existing AUTO block"}
            </pre>
          </section>
          <section className="min-w-0 rounded-lg border border-primary/50 bg-primary/5">
            <div className="flex items-center justify-between border-b border-primary/20 px-3 py-2">
              <div className="flex items-center gap-2">
                <p className="text-sm font-medium">New candidate</p>
                {preview.fellback ? <Badge variant="warning">Fallback</Badge> : null}
              </div>
              <Button type="button" size="sm" variant="ghost" onClick={() => setMerged(preview.candidate_auto)}>
                Copy to merge <ArrowRight />
              </Button>
            </div>
            <pre className="max-h-72 overflow-auto whitespace-pre-wrap p-3 font-mono text-xs">
              {preview.candidate_auto}
            </pre>
          </section>
        </div>

        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-3">
            <p className="text-sm font-medium">Combined result</p>
            <span className="text-xs text-muted-foreground">
              Edit freely or paste selected parts from either side.
            </span>
          </div>
          <Textarea
            value={merged}
            onChange={(event) => setMerged(event.target.value)}
            className="min-h-56 font-mono text-xs"
            aria-label="Combined AUTO block"
          />
        </div>

        <DialogFooter className="flex-wrap sm:justify-between">
          <Button
            type="button"
            variant="ghost"
            disabled={apply.isPending}
            onClick={() => resolve("keep_current")}
          >
            Keep current
          </Button>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              disabled={apply.isPending}
              onClick={() => resolve("use_candidate")}
            >
              Use new
            </Button>
            <Button
              type="button"
              disabled={apply.isPending || !merged.trim()}
              onClick={() => resolve("merge", merged)}
            >
              <GitMerge /> Save combined
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
