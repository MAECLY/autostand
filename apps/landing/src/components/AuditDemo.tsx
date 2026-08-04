/**
 * The provenance section: what the audit sidecar buys you, shown rather than claimed.
 *
 * The app's real AuditViewer reads a sidecar over Tauri IPC, so it cannot run on a
 * static site. This is a self-contained retelling built from base components with
 * hardcoded rows, labelled as an example so nobody mistakes it for their own data.
 *
 * Static on purpose — no `client:*` directive, no island. The classification
 * meanings are the whole payload of the section, and meaning that only appears on
 * hover is meaning a phone never shows, so they are printed as text instead of
 * hidden behind a Tooltip.
 */
import { Badge } from "@design-system/components/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@design-system/components/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@design-system/components/table";
import { AuditPhantomIcon } from "@design-system/icons";
import { cn } from "@design-system/lib/utils";

/** The six classes `classify_bullet` can return — see docs/specs/audit.md. */
export type AuditClass = "commit" | "github" | "review" | "note" | "phantom" | "unverified";

interface Classification {
  /** What the audit calls it. Lowercase, because that is what lands in the JSON. */
  readonly label: string;
  /** One line, from docs/specs/audit.md § Classifications. */
  readonly meaning: string;
  /** Token utility. Same six colours the app's audit page paints with. */
  readonly color: string;
}

const CLASSIFICATIONS: Record<AuditClass, Classification> = {
  commit: {
    label: "commit",
    meaning: "Matches a commit in your local git history.",
    color: "text-audit-commit",
  },
  github: {
    label: "github",
    meaning: "Matches a pull request or issue read through the gh CLI.",
    color: "text-audit-github",
  },
  review: {
    label: "review",
    meaning: "Matches a pull request review you left inside the window.",
    color: "text-audit-review",
  },
  note: {
    label: "note",
    meaning: "Matches a note of yours that survived the scrub.",
    color: "text-audit-note",
  },
  phantom: {
    label: "phantom",
    meaning: "Claims code work no commit backs. The audit fails.",
    color: "text-audit-phantom",
  },
  unverified: {
    label: "unverified",
    meaning: "No matching source at all. A warning, not a failure.",
    color: "text-audit-unverified",
  },
};

const CLASSIFICATION_ORDER: readonly AuditClass[] = [
  "commit",
  "github",
  "review",
  "note",
  "phantom",
  "unverified",
];

interface DemoBullet {
  /** The bullet as it would appear inside the AUTO block. */
  readonly bullet: string;
  /** The source the classifier matched it against, or why it matched nothing. */
  readonly evidence: string;
  readonly classification: AuditClass;
}

/**
 * Illustrative rows, not a capture of anyone's real standup. The fifth is the
 * point of the section: a plausible sentence with nothing underneath it.
 */
const BULLETS: readonly DemoBullet[] = [
  {
    bullet: "Implemented the LlmAdapter trait for all five providers",
    evidence: "a3f19c2 — feat(adapters/llm): implement 5 providers",
    classification: "commit",
  },
  {
    bullet: "Opened autostand #41 — deterministic renderer fallback",
    evidence: "PR #41, opened Aug 01",
    classification: "github",
  },
  {
    bullet: "Approved autostand #38 — IPC contracts",
    evidence: "Review on #38, Aug 02",
    classification: "review",
  },
  {
    bullet: "Paired on the cron parser with the platform team",
    evidence: ".remember/now.md, Aug 02",
    classification: "note",
  },
  {
    bullet: "Fixed the retry logic in the billing worker",
    evidence: "FIF-118 — commits only on Jul 28, outside the window",
    classification: "phantom",
  },
  {
    bullet: "Drafted the provider support matrix",
    evidence: "No matching source",
    classification: "unverified",
  },
];

interface ClassBadgeProps {
  readonly classification: AuditClass;
}

function ClassBadge({ classification }: ClassBadgeProps) {
  const { label, color } = CLASSIFICATIONS[classification];

  return (
    <Badge variant="outline" className={cn("gap-1.5 font-mono", color)}>
      {/* `bg-current` so the dot always tracks the class colour above it. */}
      <span className="size-1.5 rounded-full bg-current" aria-hidden="true" />
      {label}
    </Badge>
  );
}

export interface AuditDemoProps {
  /** Anchor target, so the page nav can link straight to this section. */
  id?: string;
  /** Extra classes for the outer `<section>` (page rhythm is the page's call). */
  className?: string;
}

export function AuditDemo({ id = "audit", className }: AuditDemoProps) {
  const headingId = `${id}-title`;

  return (
    <section
      id={id}
      aria-labelledby={headingId}
      className={cn("py-20 sm:py-28", className)}
    >
      <div className="mx-auto w-full max-w-5xl px-6">
        <p className="text-sm font-medium uppercase tracking-widest text-muted-foreground">
          Provenance
        </p>
        <h2 id={headingId} className="mt-6 text-3xl font-bold tracking-tight sm:text-4xl">
          Every bullet says where it came from
        </h2>
        <p className="mt-4 max-w-2xl text-lg leading-relaxed text-muted-foreground">
          {/* Location per docs/specs/audit.md: <state_dir>/audit/<F>-<HOST>.json, 0600,
              never committed — not a file alongside the standup. */}
          Each compile writes an audit sidecar into autostand's state directory, never into
          your dailies repo. It records the commits, pull requests, reviews and notes that
          went in, so any line in the file can be traced back to the thing that produced it
          — or exposed as a line with nothing underneath it.
        </p>

        <Card className="mt-12 overflow-hidden">
          {/* px-4 rather than the card default, so the filename lines up with the
              first column header of the table below it. */}
          <CardHeader className="flex-row flex-wrap items-start justify-between gap-3 px-4 py-5">
            <div className="flex flex-col gap-1.5">
              <CardTitle className="font-mono text-base">2026-08-03.md</CardTitle>
              <CardDescription>
                AUTO block for MacStudio-de-Miguel · window Aug 01–02
              </CardDescription>
            </div>
            <Badge variant="secondary">Example data</Badge>
          </CardHeader>
          <CardContent className="px-0 pb-0">
            {/* Table ships its own overflow-auto wrapper, so the page never
                scrolls sideways when the evidence column runs long. That wrapper
                is a keyboard-reachable region: on a phone the table is wider than
                the screen, and the label is what a screen reader announces when
                focus lands there. */}
            <Table className="min-w-2xl" scrollRegionLabel="Audit rows for 2026-08-03.md, scrollable">
              <TableHeader>
                <TableRow>
                  <TableHead className="w-[45%]">Standup bullet</TableHead>
                  <TableHead className="w-[35%]">Matched source</TableHead>
                  <TableHead className="w-[20%]">Class</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {BULLETS.map((row) => {
                  const isPhantom = row.classification === "phantom";

                  return (
                    <TableRow
                      key={row.bullet}
                      // The phantom row is the one a reader must not skim past, so it
                      // gets weight and an edge as well as its badge.
                      className={cn(
                        isPhantom && "border-l-2 border-l-audit-phantom bg-muted hover:bg-muted",
                      )}
                    >
                      <TableCell
                        className={cn(
                          "align-top text-foreground",
                          isPhantom && "font-medium",
                        )}
                      >
                        {row.bullet}
                      </TableCell>
                      <TableCell className="align-top font-mono text-xs text-muted-foreground">
                        {row.evidence}
                      </TableCell>
                      <TableCell className="align-top">
                        <ClassBadge classification={row.classification} />
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </CardContent>
        </Card>

        <div className="mt-6 flex items-start gap-3 rounded-lg border border-border bg-surface p-4">
          <AuditPhantomIcon size={20} className="mt-0.5 shrink-0 text-audit-phantom" />
          <p className="text-sm leading-relaxed text-muted-foreground">
            <span className="font-medium text-foreground">
              The billing-worker line is a phantom.
            </span>{" "}
            It reads like work, but its ticket has no commit inside the window — a sentence
            carried over from a day that was already filed. One phantom fails the audit, and
            you find out before anyone else reads the standup.
          </p>
        </div>

        <h3 className="mt-14 text-lg font-semibold">What the classes mean</h3>
        <dl className="mt-5 grid gap-x-10 gap-y-4 sm:grid-cols-2">
          {CLASSIFICATION_ORDER.map((classification) => (
            <div key={classification} className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
              <dt className="shrink-0">
                <ClassBadge classification={classification} />
              </dt>
              <dd className="min-w-0 flex-1 text-sm leading-relaxed text-muted-foreground">
                {CLASSIFICATIONS[classification].meaning}
              </dd>
            </div>
          ))}
        </dl>
      </div>
    </section>
  );
}
