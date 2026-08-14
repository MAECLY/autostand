/**
 * Settings → Paths: who counts as "me" when local-git runs `git log --author`.
 *
 * `standup_authors` had no control anywhere in the app, and `AppConfig` derives
 * `Default`, so every install carried `[]`. That is the field the authoritative
 * source filters on, so an empty list is the difference between a standup made
 * of real commits and one made of nothing.
 *
 * The copy mirrors the three-step cascade in
 * `crates/autostand-adapters/src/sources/local_git.rs`: configured list →
 * this machine's git identity → a visible misconfiguration error. The identity
 * shown as a suggestion is read by the backend through the very same probe the
 * fallback uses, so what this card promises is what the pipeline will do.
 */

import { useState, type FormEvent } from "react";
import { Info, Plus, Save, TriangleAlert, X } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@autostand/ui/components/alert";
import { Button } from "@autostand/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@autostand/ui/components/collapsible";
import { Input } from "@autostand/ui/components/input";
import { Label } from "@autostand/ui/components/label";
import { Separator } from "@autostand/ui/components/separator";

import { useConfig, useSetConfig } from "@/hooks/use-config";
import { standupReadinessKey, useStandupReadiness } from "@/hooks/use-readiness";

/** `git log` ref selector applied when `git_refs` is blank. */
const DEFAULT_GIT_REFS = "--all";

interface ScanDraft {
  authors: string[];
  git_refs: string;
}

/** Trim, drop blanks and dedupe — the frontend half of `clean_authors`. */
function cleanAuthors(authors: string[]): string[] {
  const out: string[] = [];
  for (const author of authors) {
    const trimmed = author.trim();
    if (trimmed.length > 0 && !out.includes(trimmed)) out.push(trimmed);
  }
  return out;
}

function sameDraft(a: ScanDraft, b: ScanDraft): boolean {
  return (
    a.git_refs === b.git_refs &&
    a.authors.length === b.authors.length &&
    a.authors.every((author, index) => author === b.authors[index])
  );
}

export function CommitScanCard() {
  const config = useConfig();
  const setConfig = useSetConfig();
  const readiness = useStandupReadiness();
  const queryClient = useQueryClient();
  // `null` means "no local edit yet", so the fields track the loaded config
  // until the user types — no effect needed to resync after a save.
  const [draft, setDraft] = useState<ScanDraft | null>(null);
  const [pendingAuthor, setPendingAuthor] = useState("");

  if (config.isPending) {
    return <div className="h-40 animate-pulse rounded-lg bg-muted" />;
  }
  if (config.isError) {
    return (
      <Card>
        <CardContent className="pt-6 text-sm text-muted-foreground">
          Could not load settings.
        </CardContent>
      </Card>
    );
  }

  // Bound before the callbacks close over it: narrowing from the guards above
  // does not survive into a deferred closure.
  const current = config.data;
  const saved: ScanDraft = {
    authors: cleanAuthors(current.standup_authors),
    git_refs: current.git_refs,
  };
  const value = draft ?? saved;
  const dirty = !sameDraft(value, saved);
  const identityProbed = readiness.data !== undefined;
  const identity = readiness.data?.git_identity ?? null;
  const canSuggestIdentity =
    identity !== null && !value.authors.includes(identity);

  function patchDraft(changes: Partial<ScanDraft>) {
    setDraft((previous) => ({ ...(previous ?? saved), ...changes }));
  }

  function addAuthor(author: string) {
    const trimmed = author.trim();
    if (trimmed.length === 0 || value.authors.includes(trimmed)) return;
    patchDraft({ authors: [...value.authors, trimmed] });
    setPendingAuthor("");
  }

  function removeAuthor(author: string) {
    patchDraft({
      authors: value.authors.filter((entry) => entry !== author),
    });
  }

  function submitPendingAuthor(event: FormEvent) {
    event.preventDefault();
    addAuthor(pendingAuthor);
  }

  function save() {
    if (!dirty) return;
    setConfig.mutate(
      {
        ...current,
        standup_authors: value.authors,
        git_refs: value.git_refs.trim(),
      },
      {
        onSuccess: () => {
          // Drop the draft so the fields follow the reloaded config again.
          setDraft(null);
          void queryClient.invalidateQueries({ queryKey: standupReadinessKey });
        },
      },
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Commit authors</CardTitle>
        <CardDescription>
          Local Git only reports commits whose author matches one of these
          emails or names. Add every identity you commit under — work email,
          personal email, GitHub username.
        </CardDescription>
      </CardHeader>

      <CardContent className="flex flex-col gap-6">
        <div className="flex flex-col gap-2">
          {value.authors.length > 0 ? (
            <ul className="flex flex-col gap-2" aria-label="Commit authors">
              {value.authors.map((author) => (
                <li
                  key={author}
                  className="flex items-center justify-between gap-3 rounded-lg border border-border bg-inset px-3 py-2"
                >
                  <span className="truncate font-mono text-sm">{author}</span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    aria-label={`Remove ${author}`}
                    onClick={() => removeAuthor(author)}
                  >
                    <X className="size-4" aria-hidden="true" />
                  </Button>
                </li>
              ))}
            </ul>
          ) : !identityProbed ? (
            // The probe has not answered yet. Claiming there is no identity
            // would show the worst of the three messages on a guess.
            <Alert>
              <Info />
              <AlertTitle>No authors configured</AlertTitle>
              <AlertDescription>
                Local Git will fall back to this machine&apos;s git identity.
              </AlertDescription>
            </Alert>
          ) : identity === null ? (
            <Alert variant="destructive">
              <TriangleAlert />
              <AlertTitle>No author to filter on</AlertTitle>
              <AlertDescription>
                The list is empty and this machine has no{" "}
                <code className="font-mono">git config user.email</code> to fall
                back on, so Local Git cannot tell your commits from your
                teammates&apos; and will refuse to gather. Add at least one
                identity.
              </AlertDescription>
            </Alert>
          ) : (
            <Alert>
              <Info />
              <AlertTitle>Falling back to this machine&apos;s identity</AlertTitle>
              <AlertDescription>
                The list is empty, so Local Git filters commits by{" "}
                <code className="font-mono">{identity}</code> — this machine&apos;s
                git identity. Add entries here if you commit under more than one.
              </AlertDescription>
            </Alert>
          )}
        </div>

        <form className="flex items-center gap-2" onSubmit={submitPendingAuthor}>
          <Label htmlFor="standup-author-new" className="sr-only">
            Add a commit author
          </Label>
          <Input
            id="standup-author-new"
            value={pendingAuthor}
            placeholder="you@company.com"
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => setPendingAuthor(event.target.value)}
          />
          <Button
            type="submit"
            variant="outline"
            disabled={pendingAuthor.trim().length === 0}
          >
            <Plus className="size-4" aria-hidden="true" />
            Add
          </Button>
        </form>

        {canSuggestIdentity && (
          <Button
            type="button"
            variant="outline"
            className="self-start"
            onClick={() => addAuthor(identity)}
          >
            <Plus className="size-4" aria-hidden="true" />
            Use this machine&apos;s git identity ({identity})
          </Button>
        )}

        <Collapsible>
          <CollapsibleTrigger className="text-sm font-medium text-muted-foreground underline-offset-4 hover:underline">
            Advanced: scanned refs
          </CollapsibleTrigger>
          <CollapsibleContent className="flex flex-col gap-2 pt-3">
            <Label htmlFor="standup-git-refs">Git refs</Label>
            <Input
              id="standup-git-refs"
              value={value.git_refs}
              placeholder={DEFAULT_GIT_REFS}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => patchDraft({ git_refs: event.target.value })}
            />
            <p className="text-xs text-muted-foreground">
              Ref selector passed to <code className="font-mono">git log</code>.
              Leave it blank for <code className="font-mono">--all</code>: every
              branch, tag and remote. Multiple tokens are allowed (
              <code className="font-mono">--branches --tags</code>) — narrowing
              this hides commits from the standup.
            </p>
          </CollapsibleContent>
        </Collapsible>

        <Separator />

        <div className="flex items-center justify-end gap-3">
          {dirty && (
            <span className="text-sm text-muted-foreground">
              Unsaved changes
            </span>
          )}
          <Button
            type="button"
            disabled={!dirty || setConfig.isPending}
            onClick={save}
          >
            <Save aria-hidden="true" />
            {setConfig.isPending ? "Saving…" : "Save commit scan"}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
