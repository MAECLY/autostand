/**
 * Cloud-sync folder picker for the Settings → Sync tab.
 *
 * The probe walks the filesystem once; detected folders are listed in
 * preference order (platform-native first). Picking a detected folder writes
 * it to `AppConfig.dailies_dir` and leaves the git remote running as a second
 * transport for history.
 */

import { Check, Cloud, FolderOpen, RefreshCw } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";

import { useCloudFolders } from "@/hooks/use-cloud-folders";
import { useConfig, useSetConfig } from "@/hooks/use-config";
import { toAppError } from "@/lib/error";

export function SyncTab() {
  const config = useConfig();
  const setConfig = useSetConfig();
  const folders = useCloudFolders();

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

  const current = config.data;
  const activePath = current.dailies_dir;

  function pick(path: string) {
    setConfig.mutate({ ...current, dailies_dir: path });
  }

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>Cloud sync</CardTitle>
          <CardDescription>
            Point <code className="font-mono text-xs">dailies_dir</code> at a
            cloud-synced folder (iCloud Drive, OneDrive, Syncthing…) for
            instant multi-device sync. Git sync keeps running as a second
            transport for history.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Cloud className="size-4 text-muted-foreground" aria-hidden="true" />
              <span className="text-sm text-muted-foreground">
                Detected cloud folders
              </span>
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={folders.isFetching}
              onClick={() => folders.refetch()}
            >
              <RefreshCw
                className={`size-4 ${folders.isFetching ? "animate-spin" : ""}`}
                aria-hidden="true"
              />
              Re-scan
            </Button>
          </div>

          {folders.isPending ? (
            <div className="h-16 animate-pulse rounded-lg bg-muted" />
          ) : folders.isError ? (
            <p className="text-sm text-destructive">
              Could not detect cloud folders — {toAppError(folders.error).message}
            </p>
          ) : folders.data.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No cloud-sync clients detected on this machine. Configure a path
              manually in the Paths tab.
            </p>
          ) : (
            <div className="flex flex-col gap-2">
              {folders.data.map((folder) => {
                const active =
                  folder.exists && folder.path === activePath;
                return (
                  <div
                    key={folder.id}
                    className={`flex items-center justify-between rounded-lg border p-3 ${
                      active
                        ? "border-primary border-2 bg-primary/5"
                        : "border-border"
                    }`}
                  >
                    <div className="flex min-w-0 flex-col gap-1">
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-medium text-foreground">
                          {folder.label}
                        </span>
                        {active && (
                          <Badge variant="default">
                            <Check className="size-3" aria-hidden="true" />
                            Active
                          </Badge>
                        )}
                        {!folder.exists && (
                          <Badge variant="outline">Not detected</Badge>
                        )}
                      </div>
                      <code className="truncate font-mono text-xs text-muted-foreground">
                        {folder.path}
                      </code>
                      <span className="text-xs text-muted-foreground">
                        {folder.provider}
                      </span>
                    </div>
                    {folder.exists && !active && (
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={setConfig.isPending}
                        onClick={() => pick(folder.path)}
                      >
                        <FolderOpen className="size-4" aria-hidden="true" />
                        Use
                      </Button>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Current dailies directory</CardTitle>
          <CardDescription>
            The folder the pipeline writes <code className="font-mono text-xs">&lt;date&gt;.md</code> files to.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <code className="block break-all rounded-md border border-border bg-inset p-3 font-mono text-xs text-foreground">
            {activePath || "(unset — falls back to ~/Sync/Github_Dailies/dailies)"}
          </code>
        </CardContent>
      </Card>
    </div>
  );
}