/**
 * Debug: run `preview_gather` for a date and show every source verbatim,
 * before scrubbing and before rendering.
 *
 * Gather walks git and several session stores, so it stays behind an explicit
 * button rather than firing on every keystroke.
 */

import { useState } from "react";

import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { AlertTriangle, Bug, Play } from "lucide-react";

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

import { GatherPanel } from "@/components/debug/GatherPanel";
import { toAppError } from "@/lib/error";
import { tauriApi } from "@/lib/tauri";
import type { GatherPreview, NoteRef, RepoFacts } from "@/lib/types";
import { cn, formatIsoDate, todayIso } from "@/lib/utils";

export const Route = createFileRoute("/debug")({
  component: DebugPage,
});

/**
 * Local because no hooks module owns `preview_gather` yet: it is a
 * debug-only read with no cache fan-out.
 */
function useGatherPreview(date: string) {
  return useQuery({
    queryKey: ["gather-preview", date] as const,
    queryFn: () => tauriApi.previewGather(date),
    enabled: date.length > 0,
    staleTime: 0,
  });
}

function nonEmptyLines(text: string | null): string[] {
  return text === null
    ? []
    : text.split("\n").filter((line) => line.trim().length > 0);
}

/** Flatten the per-repo facts the way the pipeline prints them. */
function factsText(facts: RepoFacts[]): string {
  return facts
    .map((fact) => {
      const ticket = fact.ticket === null ? "" : ` [${fact.ticket}]`;
      const commits = fact.commits.map(
        (commit) =>
          `  ${commit.sha.slice(0, 8)} ${commit.date} ${commit.subject}` +
          (commit.files.length === 0
            ? ""
            : `\n    ${commit.files.join("\n    ")}`),
      );
      return [`${fact.repo}${ticket} — ${fact.title}`, ...commits].join("\n");
    })
    .join("\n\n");
}

function notesText(notes: NoteRef[]): string {
  return notes
    .map((note) =>
      [
        `${note.source} (${note.date})`,
        ...note.clauses.map((clause) => `  - ${clause}`),
      ].join("\n"),
    )
    .join("\n\n");
}

interface DebugSource {
  title: string;
  description: string;
  count: number;
  text: string;
}

/** Source order mirrors the gather stage: local facts, notes, then enrichment. */
function debugSources(preview: GatherPreview): DebugSource[] {
  return [
    {
      title: "FACTS",
      description: "local-git commits per repo (authoritative)",
      count: preview.facts.length,
      text: factsText(preview.facts),
    },
    {
      title: "NOTES",
      description: "note clauses per source file",
      count: preview.notes.length,
      text: notesText(preview.notes),
    },
    {
      title: "GITHUB",
      description: "pull request activity from the gh CLI",
      count: nonEmptyLines(preview.github).length,
      text: preview.github ?? "",
    },
    {
      title: "CONV",
      description: "conversation digest",
      count: nonEmptyLines(preview.conv).length,
      text: preview.conv ?? "",
    },
    {
      title: "PRREV",
      description: "pull request reviews left in the window",
      count: nonEmptyLines(preview.prrev).length,
      text: preview.prrev ?? "",
    },
    {
      title: "CLAUDE",
      description: "Claude Code session files",
      count: preview.claude_files.length,
      text: preview.claude_files.join("\n"),
    },
    {
      title: "OPENCODE",
      description: "opencode sessions",
      count: preview.opencode_sessions.length,
      text: preview.opencode_sessions.join("\n"),
    },
    {
      title: "CODEX",
      description: "Codex sessions",
      count: preview.codex_sessions.length,
      text: preview.codex_sessions.join("\n"),
    },
    {
      title: "GEMINI",
      description: "Gemini CLI sessions",
      count: preview.gemini_sessions.length,
      text: preview.gemini_sessions.join("\n"),
    },
    {
      title: "GROK",
      description: "Grok CLI sessions",
      count: preview.grok_sessions.length,
      text: preview.grok_sessions.join("\n"),
    },
  ];
}

interface TicketListProps {
  title: string;
  hint: string;
  tickets: string[];
  tone: string;
}

function TicketList({ title, hint, tickets, tone }: TicketListProps) {
  return (
    <div className="space-y-2">
      <p className="text-sm font-medium text-foreground">{title}</p>
      <p className="text-xs text-muted-foreground">{hint}</p>
      {tickets.length === 0 ? (
        <p className="rounded-md border border-border bg-inset px-3 py-3 text-sm text-muted-foreground">
          None.
        </p>
      ) : (
        <div className="flex flex-wrap gap-1.5">
          {tickets.map((ticket) => (
            <Badge key={ticket} variant="outline" className={cn("font-mono", tone)}>
              {ticket}
            </Badge>
          ))}
        </div>
      )}
    </div>
  );
}

function DebugPage() {
  const [dateInput, setDateInput] = useState(todayIso());
  const [requestedDate, setRequestedDate] = useState("");

  const preview = useGatherPreview(requestedDate);
  const data = preview.data;
  const sources = data === undefined ? [] : debugSources(data);
  const gathered = sources.reduce((total, source) => total + source.count, 0);

  return (
    <div className="min-h-0 space-y-6 p-6">
      <div className="flex flex-wrap items-end gap-4">
        <div className="space-y-1.5">
          <Label htmlFor="debug-date">Filing date</Label>
          <Input
            id="debug-date"
            type="date"
            value={dateInput}
            className="w-48"
            onChange={(event) => setDateInput(event.target.value)}
          />
        </div>
        <Button
          type="button"
          disabled={dateInput.length === 0 || preview.isFetching}
          onClick={() => setRequestedDate(dateInput)}
        >
          <Play aria-hidden />
          {preview.isFetching ? "Gathering…" : "Preview gather"}
        </Button>
        <p className="pb-2 text-sm text-muted-foreground">
          Read-only: nothing is scrubbed, rendered or written.
        </p>
      </div>

      {requestedDate.length === 0 ? (
        <Alert>
          <Bug />
          <AlertTitle>Nothing previewed yet</AlertTitle>
          <AlertDescription>
            Pick a filing date and run a preview to see the raw inputs the
            pipeline would use.
          </AlertDescription>
        </Alert>
      ) : preview.isPending ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner size="sm" label="Gathering" />
          Gathering sources for {requestedDate}…
        </div>
      ) : preview.isError ? (
        <Alert variant="destructive">
          <AlertTriangle />
          <AlertTitle>Gather preview failed</AlertTitle>
          <AlertDescription>
            {toAppError(preview.error).message}
          </AlertDescription>
        </Alert>
      ) : (
        <div className="space-y-6">
          <div className="flex flex-wrap items-center gap-2 text-sm">
            <Badge variant="outline" className="font-mono">
              {preview.data.date}
            </Badge>
            <Badge variant="outline" className="font-mono">
              {preview.data.host}
            </Badge>
            <span className="text-muted-foreground">
              window{" "}
              {preview.data.window.range_start && preview.data.window.range_end
                ? `${formatIsoDate(preview.data.window.range_start)} – ${formatIsoDate(preview.data.window.range_end)}`
                : "not set"}
            </span>
          </div>

          {gathered === 0 && (
            <Alert variant="warning">
              <AlertTriangle />
              <AlertTitle>Every source came back empty</AlertTitle>
              <AlertDescription>
                The gather stage is not wired in this build:{" "}
                <span className="font-mono">preview_gather</span> returns the
                contract shape with empty fields, so the panels below stay empty
                until it lands.
              </AlertDescription>
            </Alert>
          )}

          <section className="space-y-2">
            {sources.map((source) => (
              <GatherPanel
                key={source.title}
                title={source.title}
                description={source.description}
                count={source.count}
                text={source.text}
              />
            ))}
          </section>

          <section className="grid gap-6 md:grid-cols-2">
            <TicketList
              title="Forbidden tickets"
              hint="Anti-backdating: code-change bullets on these are phantoms."
              tickets={preview.data.forbidden_tickets}
              tone="text-audit-phantom"
            />
            <TicketList
              title="Covered tickets"
              hint="Backed by a fact or a surviving note clause."
              tickets={preview.data.covered_tickets}
              tone="text-audit-commit"
            />
          </section>

          <section className="space-y-3">
            <h2 className="text-sm font-semibold text-foreground">Skew</h2>
            {preview.data.skew.length === 0 ? (
              <p className="rounded-md border border-border bg-inset px-3 py-4 text-sm text-muted-foreground">
                No skew: every note date lined up with its commit days.
              </p>
            ) : (
              <div className="overflow-x-auto rounded-lg border border-border">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Ticket</TableHead>
                      <TableHead>Note date</TableHead>
                      <TableHead>Commit days</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {preview.data.skew.map((record) => (
                      <TableRow key={`${record.ticket}-${record.note_date}`}>
                        <TableCell className="font-mono text-xs">
                          {record.ticket}
                        </TableCell>
                        <TableCell>{formatIsoDate(record.note_date)}</TableCell>
                        <TableCell className="font-mono text-xs text-muted-foreground">
                          {record.commit_days.join(", ") || "—"}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            )}
          </section>
        </div>
      )}
    </div>
  );
}
