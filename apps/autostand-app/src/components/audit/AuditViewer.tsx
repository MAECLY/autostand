/**
 * Read-only view of one audit sidecar — the provenance JSON written next to
 * every render (`docs/specs/audit.md` § `AuditData` schema).
 *
 * Everything here is what the pipeline recorded, in the order the pipeline
 * produced it: window, facts, notes, enrichment, ticket policy, skew, and the
 * raw document itself as the last word.
 */

import type { ReactNode } from "react";

import { ChevronRight } from "lucide-react";

import { Badge } from "@design-system/components/badge";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@design-system/components/collapsible";
import { ScrollArea } from "@design-system/components/scroll-area";
import { Separator } from "@design-system/components/separator";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@design-system/components/table";

import { AuditBadge } from "@/components/audit/AuditBadge";
import { GatherPanel } from "@/components/debug/GatherPanel";
import type { AuditData } from "@/lib/types";
import { cn, formatIsoDate, formatRelative } from "@/lib/utils";

/** Enrichment blocks arrive as one text blob; count their non-empty lines. */
function nonEmptyLines(text: string | null): string[] {
  return text === null
    ? []
    : text.split("\n").filter((line) => line.trim().length > 0);
}

interface EnrichmentSource {
  title: string;
  description: string;
  count: number;
  text: string;
}

function enrichmentSources(audit: AuditData): EnrichmentSource[] {
  const github = nonEmptyLines(audit.github);
  const conv = nonEmptyLines(audit.conv);
  const prrev = nonEmptyLines(audit.prrev);

  return [
    {
      title: "GITHUB",
      description: "Pull request activity from the gh CLI",
      count: github.length,
      text: audit.github ?? "",
    },
    {
      title: "CONV",
      description: "Redacted conversation digest",
      count: conv.length,
      text: audit.conv ?? "",
    },
    {
      title: "PRREV",
      description: "Pull request reviews left in the window",
      count: prrev.length,
      text: audit.prrev ?? "",
    },
    {
      title: "CLAUDE",
      description: "Claude Code session files",
      count: audit.claude_files.length,
      text: audit.claude_files.join("\n"),
    },
    {
      title: "OPENCODE",
      description: "opencode sessions",
      count: audit.opencode_sessions.length,
      text: audit.opencode_sessions.join("\n"),
    },
    {
      title: "CODEX",
      description: "Codex sessions",
      count: audit.codex_sessions.length,
      text: audit.codex_sessions.join("\n"),
    },
    {
      title: "GEMINI",
      description: "Gemini CLI sessions",
      count: audit.gemini_sessions.length,
      text: audit.gemini_sessions.join("\n"),
    },
    {
      title: "GROK",
      description: "Grok CLI sessions",
      count: audit.grok_sessions.length,
      text: audit.grok_sessions.join("\n"),
    },
  ];
}

interface SectionProps {
  title: string;
  /** Right-aligned provenance tag or count. */
  aside?: ReactNode;
  children: ReactNode;
}

function Section({ title, aside, children }: SectionProps) {
  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-semibold text-foreground">{title}</h3>
        {aside}
      </div>
      {children}
    </section>
  );
}

interface EmptyRowProps {
  children: ReactNode;
}

function EmptyRow({ children }: EmptyRowProps) {
  return (
    <p className="rounded-md border border-border bg-inset px-3 py-4 text-sm text-muted-foreground">
      {children}
    </p>
  );
}

interface FieldProps {
  label: string;
  children: ReactNode;
  className?: string;
}

function Field({ label, children, className }: FieldProps) {
  return (
    <div className={cn("min-w-0 space-y-0.5", className)}>
      <dt className="text-xs font-medium uppercase tracking-wide text-subtle">
        {label}
      </dt>
      <dd className="truncate text-sm text-foreground">{children}</dd>
    </div>
  );
}

export interface AuditViewerProps {
  audit: AuditData;
  className?: string;
}

export function AuditViewer({ audit, className }: AuditViewerProps) {
  const sources = enrichmentSources(audit);
  const windowStart = audit.window.range_start;
  const windowEnd = audit.window.range_end;

  return (
    <div className={cn("space-y-6", className)}>
      <header className="space-y-4 rounded-lg border border-border bg-surface p-4">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-base font-semibold text-foreground">
            {audit.file || "unknown date"}
          </h2>
          <Badge variant="outline" className="font-mono">
            {audit.host || "unknown host"}
          </Badge>
          <Badge variant={audit.fellback ? "warning" : "secondary"}>
            {audit.render_used}
          </Badge>
          {audit.fellback && <Badge variant="warning">fell back</Badge>}
          <span className="ml-auto text-sm text-muted-foreground">
            {audit.rendered_at ? formatRelative(audit.rendered_at) : "never"}
          </span>
        </div>

        <Separator />

        <dl className="grid grid-cols-2 gap-4 md:grid-cols-4">
          <Field label="Window">
            {windowStart && windowEnd
              ? `${formatIsoDate(windowStart)} – ${formatIsoDate(windowEnd)}`
              : "—"}
          </Field>
          <Field label="Render mode">{audit.render_mode}</Field>
          <Field label="Provider">
            {audit.provider ?? "—"}
            {audit.model !== null && (
              <span className="text-muted-foreground"> · {audit.model}</span>
            )}
          </Field>
          <Field label="Accumulated">
            {audit.accumulated_count} bullet
            {audit.accumulated_count === 1 ? "" : "s"}
          </Field>
          <Field label="Inputs hash" className="col-span-2">
            <span className="font-mono text-xs text-muted-foreground">
              {audit.hash || "—"}
            </span>
          </Field>
        </dl>
      </header>

      <Section
        title="Facts"
        aside={<AuditBadge classification="commit" size="sm" />}
      >
        {audit.facts.length === 0 ? (
          <EmptyRow>No git facts were recorded for this window.</EmptyRow>
        ) : (
          <div className="overflow-x-auto rounded-lg border border-border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Repo</TableHead>
                  <TableHead>Ticket</TableHead>
                  <TableHead>Title</TableHead>
                  <TableHead>Commits</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {audit.facts.map((fact) => (
                  <TableRow key={fact.repo} className="align-top">
                    <TableCell className="font-medium">{fact.repo}</TableCell>
                    <TableCell className="font-mono text-xs">
                      {fact.ticket ?? "—"}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {fact.title || "—"}
                    </TableCell>
                    <TableCell>
                      <ul className="space-y-1">
                        {fact.commits.map((commit) => (
                          <li
                            key={commit.sha}
                            className="font-mono text-xs text-muted-foreground"
                          >
                            <span className="text-foreground">
                              {commit.sha.slice(0, 8)}
                            </span>{" "}
                            {commit.subject}
                          </li>
                        ))}
                        {fact.commits.length === 0 && (
                          <li className="text-xs text-subtle">none</li>
                        )}
                      </ul>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </Section>

      <Section
        title="Notes"
        aside={<AuditBadge classification="note" size="sm" />}
      >
        {audit.notes.length === 0 ? (
          <EmptyRow>No note clauses survived the scrub for this window.</EmptyRow>
        ) : (
          <ul className="space-y-3">
            {audit.notes.map((note) => (
              <li
                key={`${note.source}-${note.date}`}
                className="rounded-lg border border-border bg-surface p-3"
              >
                <div className="flex items-baseline justify-between gap-3">
                  <span className="truncate font-mono text-xs text-muted-foreground">
                    {note.source}
                  </span>
                  <span className="shrink-0 text-xs text-subtle">
                    {formatIsoDate(note.date)}
                  </span>
                </div>
                <ul className="mt-2 list-disc space-y-1 pl-5 text-sm text-foreground">
                  {note.clauses.map((clause) => (
                    <li key={clause}>{clause}</li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section
        title="Enrichment"
        aside={
          <span className="flex items-center gap-2">
            <AuditBadge classification="github" size="sm" />
            <AuditBadge classification="review" size="sm" />
          </span>
        }
      >
        {/* The enrichment blocks are the same raw source text the Debug page
            shows, so they reuse the same panel. */}
        <div className="space-y-2">
          {sources.map((source) => (
            <GatherPanel
              key={source.title}
              title={source.title}
              description={source.description}
              count={source.count}
              text={source.text}
            />
          ))}
        </div>
      </Section>

      <Section
        title="Tickets"
        aside={<AuditBadge classification="phantom" size="sm" />}
      >
        <div className="grid gap-4 md:grid-cols-2">
          <div className="space-y-2">
            <p className="text-xs font-medium uppercase tracking-wide text-subtle">
              Forbidden — a code-change bullet here is a phantom
            </p>
            {audit.forbidden_tickets.length === 0 ? (
              <EmptyRow>None.</EmptyRow>
            ) : (
              <div className="flex flex-wrap gap-1.5">
                {audit.forbidden_tickets.map((ticket) => (
                  <Badge
                    key={ticket}
                    variant="outline"
                    className="font-mono text-audit-phantom"
                  >
                    {ticket}
                  </Badge>
                ))}
              </div>
            )}
          </div>
          <div className="space-y-2">
            <p className="text-xs font-medium uppercase tracking-wide text-subtle">
              Covered — backed by a fact or a note
            </p>
            {audit.covered_tickets.length === 0 ? (
              <EmptyRow>None.</EmptyRow>
            ) : (
              <div className="flex flex-wrap gap-1.5">
                {audit.covered_tickets.map((ticket) => (
                  <Badge
                    key={ticket}
                    variant="outline"
                    className="font-mono text-audit-commit"
                  >
                    {ticket}
                  </Badge>
                ))}
              </div>
            )}
          </div>
        </div>
      </Section>

      <Section title="Skew">
        {audit.skew.length === 0 ? (
          <EmptyRow>
            No skew: every note date lined up with its commit days.
          </EmptyRow>
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
                {audit.skew.map((record) => (
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
      </Section>

      <Collapsible className="overflow-hidden rounded-lg border border-border bg-surface">
        <CollapsibleTrigger className="group flex w-full items-center gap-3 px-3 py-2.5 text-left transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
          <ChevronRight
            aria-hidden
            className="size-4 shrink-0 text-muted-foreground transition-transform group-data-[state=open]:rotate-90"
          />
          <span className="flex-1 text-sm font-medium text-foreground">
            Sidecar JSON
          </span>
          <Badge variant="outline" className="font-mono">
            {audit.file || "sidecar"}.json
          </Badge>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="border-t border-border">
            <ScrollArea className="max-h-96">
              {/* Wrapped rather than scrolled sideways: the scroll area only
                  ships a vertical bar, so a long line would hide itself. */}
              <pre className="whitespace-pre-wrap break-words px-3 py-3 font-mono text-xs leading-relaxed text-foreground">
                {JSON.stringify(audit, null, 2)}
              </pre>
            </ScrollArea>
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}
