/**
 * Dashboard — focused tabs for today's standup, manual items, and the pipeline.
 * The TerminalViewer lives in the global bottom panel (__root.tsx).
 */

import { useState } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { AlertTriangle, Inbox, RefreshCw } from "lucide-react";

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
import { Button } from "@autostand/ui/components/button";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@autostand/ui/components/tabs";

import { CompileButton } from "@/components/standup/CompileButton";
import { ManualEditor } from "@/components/standup/ManualEditor";
import { PipelineCard } from "@/components/standup/PipelineCard";
import { StandupPreview } from "@/components/standup/StandupPreview";
import { useHostSlug } from "@/hooks/use-config";
import { usePipelineStatus } from "@/hooks/use-pipeline-status";
import { useSchedulerStatus } from "@/hooks/use-scheduler";
import { useStandupFile } from "@/hooks/use-standup";
import { toAppError } from "@/lib/error";
import { formatIsoDate, todayIso } from "@/lib/utils";
import type { TriggerSource } from "@/lib/types";

export const Route = createFileRoute("/")({
  component: DashboardPage,
});

const SKELETON_LINES = ["w-full", "w-11/12", "w-3/4"] as const;

const TRIGGER_LABELS: Record<TriggerSource, string> = {
  scheduled: "Scheduled",
  manual: "Manual",
  "self-heal": "Self-heal",
};

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
          Use Compile now to gather commits, notes and enrichment for the
          window and write the AUTO block. Manual items can be added before or
          after.
        </CardDescription>
      </CardHeader>
    </Card>
  );
}

interface TodayStandupProps {
  date: string;
  regenerating: boolean;
}

function TodayStandup({ date, regenerating }: TodayStandupProps) {
  const { data: hostSlug } = useHostSlug();
  const standup = useStandupFile(date);
  const scheduler = useSchedulerStatus();
  const [retryNonce, setRetryNonce] = useState(0);

  if (standup.isPending) return <PreviewSkeleton />;

  if (standup.isError) {
    const error = toAppError(standup.error);
    if (error.code === "not_found") return <EmptyToday date={date} />;

    return (
      <Alert variant="destructive">
        <AlertTriangle />
        <AlertTitle>Could not read today&apos;s standup</AlertTitle>
        <AlertDescription>
          <p className="font-mono text-xs">{error.code}</p>
          <p>{error.message}</p>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="mt-3"
            onClick={() => {
              setRetryNonce((n) => n + 1);
              void standup.refetch();
            }}
            data-retry-nonce={retryNonce}
          >
            <RefreshCw className="size-4" aria-hidden="true" />
            Retry
          </Button>
        </AlertDescription>
      </Alert>
    );
  }

  const lastTrigger = scheduler.data?.last_trigger ?? null;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        {lastTrigger ? (
          <span>
            Last run:{" "}
            <span className="font-medium text-foreground">
              {TRIGGER_LABELS[lastTrigger]}
            </span>
          </span>
        ) : null}
        {regenerating ? (
          <span className="animate-pulse text-warning-fg">
            Regenerating…
          </span>
        ) : null}
      </div>
      <div className={regenerating ? "opacity-60" : undefined}>
        <StandupPreview content={standup.data} hostSlug={hostSlug} />
      </div>
    </div>
  );
}

type DashboardTab = "today" | "manual" | "pipeline";

function DashboardPage() {
  const date = todayIso();
  const [tab, setTab] = useState<DashboardTab>("today");
  const pipeline = usePipelineStatus();

  const isCompiling =
    pipeline.data?.state === "gathering" ||
    pipeline.data?.state === "rendering";

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
        <CompileButton />
      </header>

      <Tabs
        value={tab}
        onValueChange={(v) => setTab(v as DashboardTab)}
        className="min-h-0"
      >
        <TabsList>
          <TabsTrigger value="today">Today</TabsTrigger>
          <TabsTrigger value="manual">Manual item</TabsTrigger>
          <TabsTrigger value="pipeline">Pipeline</TabsTrigger>
        </TabsList>

        <TabsContent value="today" className="min-h-0">
          <TodayStandup date={date} regenerating={isCompiling} />
        </TabsContent>

        <TabsContent value="manual" className="min-h-0">
          <ManualEditor date={date} />
        </TabsContent>

        <TabsContent value="pipeline" className="min-h-0">
          <PipelineCard />
        </TabsContent>
      </Tabs>
    </div>
  );
}