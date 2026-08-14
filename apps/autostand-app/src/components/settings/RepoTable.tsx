/**
 * Settings → Paths: the repos `discover_repos` finds under `github_dir`.
 *
 * Discovery walks the filesystem, so it stays behind an explicit button rather
 * than running on mount.
 */

import { FolderSearch, RefreshCw } from "lucide-react";

import { Button } from "@autostand/ui/components/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@autostand/ui/components/table";

import { OpenPathButton } from "@/components/common/OpenPathButton";
import { useDiscoverRepos } from "@/hooks/use-paths";
import { formatRelative } from "@/lib/utils";

export function RepoTable() {
  const discoverRepos = useDiscoverRepos();
  const repos = discoverRepos.data;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-4">
        <div>
          <p className="text-sm font-medium text-foreground">Repositories</p>
          <p className="text-sm text-muted-foreground">
            Git repos found one level below the configured GitHub directory.
          </p>
        </div>
        <Button
          type="button"
          variant="outline"
          disabled={discoverRepos.isPending}
          onClick={() => discoverRepos.mutate()}
        >
          <RefreshCw className="size-4" aria-hidden="true" />
          {discoverRepos.isPending ? "Discovering…" : "Discover repos"}
        </Button>
      </div>

      {repos === undefined || repos.length === 0 ? (
        <div className="flex flex-col items-center gap-2 rounded-lg border border-border bg-inset px-6 py-10 text-center">
          <FolderSearch
            className="size-6 text-muted-foreground"
            aria-hidden="true"
          />
          <p className="text-sm text-muted-foreground">
            {repos === undefined
              ? "Run discovery to list the repos autostand will read commits from."
              : "No git repos found under the configured GitHub directory."}
          </p>
        </div>
      ) : (
        <div className="overflow-x-auto rounded-lg border border-border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Path</TableHead>
                <TableHead>Remote</TableHead>
                <TableHead>Last commit</TableHead>
                <TableHead>
                  <span className="sr-only">Actions</span>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {repos.map((repo) => (
                <TableRow key={repo.path}>
                  <TableCell className="font-medium">{repo.name}</TableCell>
                  <TableCell
                    className="max-w-xs truncate font-mono text-xs text-muted-foreground"
                    title={repo.path}
                  >
                    {repo.path}
                  </TableCell>
                  <TableCell
                    className="max-w-xs truncate font-mono text-xs text-muted-foreground"
                    title={repo.remote ?? undefined}
                  >
                    {repo.remote ?? "—"}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {repo.last_commit_at === null
                      ? "—"
                      : formatRelative(repo.last_commit_at)}
                  </TableCell>
                  <TableCell className="text-right">
                    <OpenPathButton
                      path={repo.path}
                      label={`Open ${repo.name} in the file manager`}
                    />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  );
}
