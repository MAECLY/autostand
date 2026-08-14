/** Settings → Sync: cloud folder plus optional private GitHub history. */

import { useState } from "react";
import {
  Check,
  Cloud,
  CloudOff,
  FolderOpen,
  GitBranch,
  Github,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";
import { Input } from "@autostand/ui/components/input";
import { Label } from "@autostand/ui/components/label";

import { OpenPathButton } from "@/components/common/OpenPathButton";
import { DependencyChecklist } from "@/components/settings/DependencyChecklist";
import { useCloudFolders } from "@/hooks/use-cloud-folders";
import { useConfig } from "@/hooks/use-config";
import {
  useConfigureCloudSync,
  useRepoSyncStatus,
  useSetupRepoSync,
} from "@/hooks/use-sync";
import { toAppError } from "@/lib/error";

export function SyncTab() {
  const config = useConfig();
  const folders = useCloudFolders();
  const configureCloud = useConfigureCloudSync();
  const repo = useRepoSyncStatus();
  const setupRepo = useSetupRepoSync();
  const [repoName, setRepoName] = useState("autostand-dailies");

  if (config.isPending) return <div className="h-20 animate-pulse rounded-lg bg-muted" />;
  if (config.isError) {
    const appError = toAppError(config.error);
    return (
      <Card>
        <CardContent className="pt-6 text-sm text-destructive">
          Could not load settings — {appError.code}: {appError.message}
        </CardContent>
      </Card>
    );
  }

  const activeRoot = config.data.sync.cloud_root;

  return (
    <div className="grid items-start gap-4 xl:grid-cols-[minmax(0,1.2fr)_minmax(320px,0.8fr)]">
      <Card>
        <CardHeader>
          <div className="flex items-start justify-between gap-4">
            <div>
              <CardTitle className="flex items-center gap-2">
                <Cloud className="size-4" aria-hidden="true" /> Cloud Sync
              </CardTitle>
              <CardDescription className="mt-1 max-w-2xl">
                Pick a cloud provider. Autostand creates and uses its own
                <code className="mx-1 font-mono text-xs">autostand</code>
                folder, keeping your cloud root tidy.
              </CardDescription>
            </div>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={folders.isFetching}
              onClick={() => folders.refetch()}
            >
              <RefreshCw className={folders.isFetching ? "animate-spin" : ""} />
              Scan
            </Button>
          </div>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {folders.isPending ? (
            <div className="h-20 animate-pulse rounded-lg bg-muted" />
          ) : folders.isError ? (
            <p className="text-sm text-destructive">
              Could not detect cloud folders — {toAppError(folders.error).message}
            </p>
          ) : folders.data.length === 0 ? (
            <div className="flex gap-3 rounded-lg bg-muted/50 p-4 text-sm text-muted-foreground">
              <CloudOff className="size-4 shrink-0" aria-hidden="true" />
              No supported cloud client was detected. You can still configure a
              directory from Paths.
            </div>
          ) : (
            folders.data.map((folder) => {
              const active = folder.exists && folder.path === activeRoot;
              return (
                <div
                  key={folder.id}
                  className={`flex items-center justify-between gap-4 rounded-lg border p-3 ${
                    active ? "border-primary bg-primary/5" : "border-border"
                  }`}
                >
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-sm font-medium">{folder.label}</span>
                      {active ? (
                        <Badge variant="default"><Check /> Active</Badge>
                      ) : null}
                      {!folder.exists ? <Badge variant="outline">Not detected</Badge> : null}
                    </div>
                    <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                      {folder.dailies_path}
                    </p>
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    {folder.exists && !active ? (
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={configureCloud.isPending}
                        onClick={() => configureCloud.mutate(folder.path)}
                      >
                        <FolderOpen /> Use
                      </Button>
                    ) : null}
                    <OpenPathButton
                      path={active ? folder.dailies_path : folder.path}
                      label={`Open ${folder.label} in the file manager`}
                      disabled={!folder.exists}
                    />
                  </div>
                </div>
              );
            })
          )}

          <div className="flex items-start justify-between gap-2 rounded-lg bg-inset p-3">
            <div className="min-w-0">
              <p className="text-xs font-medium text-muted-foreground">Standup location</p>
              <code className="mt-1 block break-all font-mono text-xs">
                {config.data.dailies_dir || "Not configured"}/YYYY-MM-DD.md
              </code>
            </div>
            <OpenPathButton
              path={config.data.dailies_dir}
              label="Open the standup folder in the file manager"
            />
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Github className="size-4" aria-hidden="true" /> Repo Sync
          </CardTitle>
          <CardDescription>
            Optional private GitHub history for the same Cloud Sync folder. Both
            transports can run together; no second copy is created.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {/* Outside the status branch on purpose: when `get_repo_sync_status`
              itself fails, the missing prerequisite is usually the reason. */}
          <DependencyChecklist
            group="repo_sync"
            description="Repo Sync needs all three before it can create or push a private repository."
          />

          {repo.isPending ? (
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
          ) : repo.isError ? (
            <p className="text-sm text-destructive">{toAppError(repo.error).message}</p>
          ) : (
            <>
              {repo.data.enabled ? (
                <div className="rounded-lg bg-success-bg p-3 text-sm text-success">
                  <p className="flex items-center gap-2 font-medium">
                    <ShieldCheck className="size-4" /> Private sync active
                  </p>
                  <p className="mt-1 font-mono text-xs">
                    {repo.data.repository ?? repo.data.repo_path}
                  </p>
                </div>
              ) : (
                <>
                  {repo.data.message ? (
                    <p className="rounded-lg bg-muted/50 p-3 text-sm text-muted-foreground">
                      {repo.data.message}
                    </p>
                  ) : null}
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="repo-sync-name">Private repository name</Label>
                    <Input
                      id="repo-sync-name"
                      value={repoName}
                      disabled={!repo.data.can_setup || setupRepo.isPending}
                      onChange={(event) => setRepoName(event.target.value)}
                    />
                  </div>
                  <Button
                    type="button"
                    disabled={!repo.data.can_setup || setupRepo.isPending || !repoName.trim()}
                    onClick={() => setupRepo.mutate(repoName)}
                  >
                    <GitBranch /> Create private repo &amp; enable
                  </Button>
                  <p className="text-xs text-muted-foreground">
                    This initializes Git in the active standup folder, creates a
                    private GitHub repository, and pushes only after a compile.
                  </p>
                </>
              )}
            </>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
