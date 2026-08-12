/**
 * Dashboard — today's standup, the single "Compile now" action, and the live
 * pipeline card. Spec: `docs/tauri/04-frontend-stack.md` § Key pages.
 */

import { createFileRoute } from "@tanstack/react-router";
import { AlertTriangle, Inbox } from "lucide-react";

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@autostand/ui/components/alert";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";

import { CompileButton } from "@/components/standup/CompileButton";
import { ManualEditor } from "@/components/standup/ManualEditor";
import { PipelineCard } from "@/components/standup/PipelineCard";
import { StandupPreview } from "@/components/standup/StandupPreview";
import { useHostSlug } from "@/hooks/use-config";
import { useStandupFile } from "@/hooks/use-standup";
import { toAppError } from "@/lib/error";
import { formatIsoDate, todayIso } from "@/lib/utils";

export const Route = createFileRoute("/")({
  component: DashboardPage,
});

/** Widths of the placeholder lines, so the skeleton reads as prose. */
const SKELETON_LINES = ["w-full", "w-11/12", "w-3/4"] as const;

function PreviewSkeleton() {
  return (
    <div
      className="flex flex-col gap-4"
      aria-busy="true"
      aria-label="Loading today's standup"
    >
      <div className="h-6 w-64 animate-pulse rounded-md bg-muted" />
      <Card>
        <CardHeader>
          <div className="h-4 w-24 animate-pulse rounded-sm bg-muted" />
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          {SKELETON_LINES.map((width) => (
            <div
              key={width}
              className={`h-4 ${width} animate-pulse rounded-sm bg-muted`}
            />
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

interface EmptyTodayProps {
  date: string;
}

function EmptyToday({ date }: EmptyTodayProps) {
  return (
    <Card>
      <CardHeader className="items-center gap-2 text-center">
        <Inbox className="size-6 text-muted-foreground" aria-hidden="true" />
        <CardTitle>No standup filed for {formatIsoDate(date)}</CardTitle>
        <CardDescription>
          Compiling gathers commits, notes and enrichment for the window and
          writes the AUTO block. Manual items can be added before or after.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex justify-center">
        <CompileButton />
      </CardContent>
    </Card>
  );
}

interface TodayStandupProps {
  date: string;
}

function TodayStandup({ date }: TodayStandupProps) {
  const { data: hostSlug } = useHostSlug();
  const standup = useStandupFile(date);

  if (standup.isPending) return <PreviewSkeleton />;

  if (standup.isError) {
    const error = toAppError(standup.error);
    // `read_standup_file` rejects with `not_found` until the day's first
    // compile writes the file — that is an empty state, not a failure.
    if (error.code === "not_found") return <EmptyToday date={date} />;

    return (
      <Alert variant="destructive">
        <AlertTriangle />
        <AlertTitle>Could not read today&apos;s standup</AlertTitle>
        <AlertDescription>
          <p className="font-mono text-xs">{error.code}</p>
          <p>{error.message}</p>
        </AlertDescription>
      </Alert>
    );
  }

  return <StandupPreview content={standup.data} hostSlug={hostSlug} />;
}

function DashboardPage() {
  const date = todayIso();

  return (
    <div className="flex min-h-0 flex-col gap-6 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold text-foreground">
            Today — {formatIsoDate(date)}
          </h2>
          <p className="text-sm text-muted-foreground">
            Filed as <span className="font-mono">{date}</span> in the dailies
            directory.
          </p>
        </div>
        {/* TopBar takes page actions as children, but routes render inside its
            sibling <Outlet>, so the primary action sits in the page header. */}
        <CompileButton />
      </header>

      <div className="grid min-h-0 min-w-0 content-start gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
        <div className="flex min-h-0 min-w-0 flex-col gap-6">
          <TodayStandup date={date} />
          <ManualEditor date={date} />
        </div>

        <div className="flex min-h-0 min-w-0 flex-col gap-6">
          <PipelineCard />
        </div>
      </div>
    </div>
  );
}
