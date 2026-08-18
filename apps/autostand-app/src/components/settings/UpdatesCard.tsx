/**
 * Updating without going back to GitHub.
 *
 * Three states, and the card only ever shows one: nothing checked yet, up to
 * date, or a version waiting with its release notes. Downloading replaces the
 * button with a progress bar, and installing ends at the one action that
 * finishes the job — restarting into the new version.
 */

import { useState } from "react";
import { Download, RefreshCw, RotateCw } from "lucide-react";

import { Button } from "@autostand/ui/components/button";
import { Card, CardContent } from "@autostand/ui/components/card";
import { Progress } from "@autostand/ui/components/progress";

import { useInstallUpdate, useUpdateCheck } from "@/hooks/use-updater";

export function UpdatesCard({ currentVersion }: { readonly currentVersion: string }) {
  const check = useUpdateCheck();
  const install = useInstallUpdate();
  const [installed, setInstalled] = useState(false);

  return (
    <Card>
      <CardContent className="flex flex-col gap-4 pt-6">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <span className="text-sm text-muted-foreground">
            You are running{" "}
            <span className="font-mono text-foreground">{currentVersion}</span>.
          </span>
          <Button
            variant="outline"
            onClick={() => void check.refetch()}
            disabled={check.isFetching || install.isPending}
          >
            <RefreshCw
              className={check.isFetching ? "size-4 animate-spin" : "size-4"}
              aria-hidden="true"
            />
            {check.isFetching ? "Checking…" : "Check for updates"}
          </Button>
        </div>

        {check.isError ? (
          <p className="text-sm text-destructive">
            Could not reach the update feed. {String(check.error)}
          </p>
        ) : null}

        {check.data?.available === false ? (
          <p className="text-sm text-muted-foreground">
            No newer version published.
          </p>
        ) : null}

        {check.data?.available === true ? (
          <div className="flex flex-col gap-3 rounded-lg border border-border bg-inset p-4">
            <span className="text-sm font-medium text-foreground">
              Version {check.data.version} is available
            </span>
            {check.data.notes === null ? null : (
              <pre className="max-h-52 overflow-auto whitespace-pre-wrap font-sans text-sm text-muted-foreground">
                {check.data.notes}
              </pre>
            )}

            {install.progress === null && !install.isPending && !installed ? (
              <Button
                className="self-start"
                onClick={() =>
                  install.mutate(undefined, {
                    onSuccess: () => setInstalled(true),
                  })
                }
              >
                <Download className="size-4" aria-hidden="true" />
                Download and install
              </Button>
            ) : null}

            {install.isPending ? (
              <div className="flex flex-col gap-2">
                <Progress
                  className="h-1"
                  value={
                    install.progress === null
                      ? undefined
                      : Math.round(install.progress * 100)
                  }
                />
                <span className="font-mono text-xs text-muted-foreground">
                  {install.progress === null
                    ? "Downloading…"
                    : `${Math.round(install.progress * 100)}%`}
                </span>
              </div>
            ) : null}

            {install.isError ? (
              <p className="text-sm text-destructive">
                Install failed. {String(install.error)}
              </p>
            ) : null}

            {installed ? (
              <div className="flex flex-col gap-2">
                <p className="text-sm text-foreground">
                  Installed. autostand restarts into {check.data.version} when
                  you are ready — nothing is lost, the next compile picks up
                  where this one left off.
                </p>
                <Button className="self-start" onClick={() => void install.relaunch()}>
                  <RotateCw className="size-4" aria-hidden="true" />
                  Restart now
                </Button>
              </div>
            ) : null}
          </div>
        ) : null}

        <p className="text-xs text-muted-foreground">
          Checked only when you ask. Every bundle is signed with autostand&apos;s
          own update key and verified before it is installed, whether or not the
          build carries an OS code signature.
        </p>
      </CardContent>
    </Card>
  );
}
