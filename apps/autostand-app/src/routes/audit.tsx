/**
 * Audit: pick a filing date, pick a host's sidecar, read its provenance.
 *
 * Per-bullet classification is not computed here — the classifier lives in the
 * core crate and is not exposed over IPC yet — so the page shows the six
 * classes as a legend and the sidecar's recorded inputs as the evidence.
 */

import { useState } from "react";

import { createFileRoute } from "@tanstack/react-router";
import { FileSearch, ShieldOff } from "lucide-react";

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@autostand/ui/components/alert";
import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import { Input } from "@autostand/ui/components/input";
import { Label } from "@autostand/ui/components/label";
import { Spinner } from "@autostand/ui/components/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@autostand/ui/components/table";

import {
  AuditBadge,
  type AuditClassification,
} from "@/components/audit/AuditBadge";
import { AuditViewer } from "@/components/audit/AuditViewer";
import { useAuditSidecar, useAuditSidecars } from "@/hooks/use-audit";
import { toAppError } from "@/lib/error";
import { formatRelative, todayIso } from "@/lib/utils";

export const Route = createFileRoute("/audit")({
  component: AuditPage,
});

const LEGEND: readonly AuditClassification[] = [
  "commit",
  "github",
  "review",
  "note",
  "phantom",
  "unverified",
];

function AuditPage() {
  const [date, setDate] = useState(todayIso());
  const [pickedPath, setPickedPath] = useState<string | null>(null);

  const sidecars = useAuditSidecars(date);
  // Falling back to the first row auto-selects a freshly listed date without
  // an effect; `pickedPath` is cleared whenever the date changes.
  const activePath = pickedPath ?? sidecars.data?.[0]?.path ?? null;
  const sidecar = useAuditSidecar(activePath);

  return (
    <div className="space-y-6 p-6">
      <div className="flex flex-wrap items-end gap-4">
        <div className="space-y-1.5">
          <Label htmlFor="audit-date">Filing date</Label>
          <Input
            id="audit-date"
            type="date"
            value={date}
            className="w-48"
            onChange={(event) => {
              setDate(event.target.value);
              setPickedPath(null);
            }}
          />
        </div>
        <p className="pb-2 text-sm text-muted-foreground">
          One sidecar per host, written by each render into the state directory.
        </p>
      </div>

      <section className="space-y-3">
        <h2 className="text-sm font-semibold text-foreground">
          Classification legend
        </h2>
        <div className="flex flex-wrap gap-2">
          {LEGEND.map((classification) => (
            <AuditBadge key={classification} classification={classification} />
          ))}
        </div>
        <p className="text-xs text-muted-foreground">
          Bullet-level classification is computed by the core auditor; until it
          is exposed over IPC this page shows the sidecar inputs those rules run
          against.
        </p>
      </section>

      {date.length === 0 ? (
        <Alert>
          <FileSearch />
          <AlertTitle>Pick a filing date</AlertTitle>
          <AlertDescription>
            Sidecars are listed per filing date, one file per host.
          </AlertDescription>
        </Alert>
      ) : sidecars.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner size="sm" label="Loading sidecars" />
          Listing sidecars for {date}…
        </div>
      ) : sidecars.isError ? (
        <Alert variant="destructive">
          <ShieldOff />
          <AlertTitle>Could not list sidecars</AlertTitle>
          <AlertDescription>
            {toAppError(sidecars.error).message}
          </AlertDescription>
        </Alert>
      ) : sidecars.data.length === 0 ? (
        <Alert>
          <FileSearch />
          <AlertTitle>No audit sidecar for {date}</AlertTitle>
          <AlertDescription>
            A sidecar is written on every render. Compile a standup for this
            date to produce one.
          </AlertDescription>
        </Alert>
      ) : (
        <div className="overflow-x-auto rounded-lg border border-border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Host</TableHead>
                <TableHead>Rendered</TableHead>
                <TableHead>Render used</TableHead>
                <TableHead className="w-24" />
              </TableRow>
            </TableHeader>
            <TableBody>
              {sidecars.data.map((entry) => {
                const isActive = entry.path === activePath;
                return (
                  <TableRow
                    key={entry.path}
                    data-state={isActive ? "selected" : undefined}
                  >
                    <TableCell className="font-mono text-xs font-medium">
                      {entry.host}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {entry.rendered_at
                        ? formatRelative(entry.rendered_at)
                        : "unknown"}
                    </TableCell>
                    <TableCell>
                      <Badge variant="secondary">{entry.render_used}</Badge>
                    </TableCell>
                    <TableCell>
                      <Button
                        type="button"
                        size="sm"
                        variant={isActive ? "secondary" : "outline"}
                        onClick={() => setPickedPath(entry.path)}
                      >
                        {isActive ? "Viewing" : "View"}
                      </Button>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </div>
      )}

      {activePath !== null &&
        (sidecar.isPending ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner size="sm" label="Loading sidecar" />
            Reading sidecar…
          </div>
        ) : sidecar.isError ? (
          <Alert variant="destructive">
            <ShieldOff />
            <AlertTitle>Could not read the sidecar</AlertTitle>
            <AlertDescription>
              {toAppError(sidecar.error).message}
            </AlertDescription>
          </Alert>
        ) : (
          <AuditViewer audit={sidecar.data} />
        ))}
    </div>
  );
}
