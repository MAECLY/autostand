/**
 * The one thing worth interrupting a first launch for.
 *
 * autostand cannot do anything useful until the OS lets it read the folders it
 * was pointed at. On macOS that consent is per-app and per-location, it is asked
 * for by touching the location, and a refusal is remembered — so the moment to
 * ask is the first launch, not the first failed compile, where the same denial
 * surfaces as "no repositories found".
 *
 * It opens only when something is actually denied. A dialog that appears on
 * every launch to report that everything is fine teaches people to dismiss
 * dialogs, and the next one that matters gets dismissed too. Windows and Linux
 * gate none of this, so it never opens there at all.
 */

import { useState } from "react";
import { FolderLock, ExternalLink, Check, X } from "lucide-react";

import { Button } from "@autostand/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@autostand/ui/components/dialog";

import { useRequestSystemAccess, useSystemAccess } from "@/hooks/use-system-access";
import { tauriApi } from "@/lib/tauri";
import type { AccessCheck } from "@/lib/types";

export function SystemAccessDialog() {
  const access = useSystemAccess();
  const request = useRequestSystemAccess();
  // Dismissal is deliberately not persisted. The dialog is driven by whether
  // access is still denied, so remembering a dismissal would mean remembering to
  // stay silent about a broken app.
  const [dismissed, setDismissed] = useState(false);

  const open = access.data?.needs_attention === true && !dismissed;
  const denied = (access.data?.checks ?? []).filter((c) => c.state === "denied");
  const settingsUrl = access.data?.settings_url ?? null;

  return (
    <Dialog open={open} onOpenChange={(next) => !next && setDismissed(true)}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <FolderLock className="size-5 text-primary" aria-hidden="true" />
            autostand cannot read your folders yet
          </DialogTitle>
          <DialogDescription>
            macOS asks before an app reads folders like Documents, Desktop or
            iCloud Drive. Until it is allowed, every standup comes back empty —
            with nothing to say why.
          </DialogDescription>
        </DialogHeader>

        <ul className="flex flex-col gap-3">
          {denied.map((check) => (
            <DeniedLocation key={check.id} check={check} />
          ))}
        </ul>

        <p className="text-xs text-muted-foreground">
          Read-only. autostand never writes outside the standup folder you chose,
          and never sends any of it anywhere unless you point it at a provider.
        </p>

        <DialogFooter className="gap-2 sm:justify-between">
          <Button variant="ghost" onClick={() => setDismissed(true)}>
            Not now
          </Button>
          <span className="flex flex-wrap gap-2">
            {settingsUrl === null ? null : (
              <Button
                variant="outline"
                onClick={() => void tauriApi.openAccessSettings()}
              >
                <ExternalLink className="size-4" aria-hidden="true" />
                Open System Settings
              </Button>
            )}
            <Button
              onClick={() => request.mutate()}
              disabled={request.isPending}
            >
              {request.isPending ? "Waiting for macOS…" : "Grant access"}
            </Button>
          </span>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** One refused location: what it is, where it is, and why it is wanted. */
function DeniedLocation({ check }: { readonly check: AccessCheck }) {
  return (
    <li className="rounded-lg border border-border bg-inset p-3">
      <span className="flex items-center gap-2 text-sm font-medium text-foreground">
        <X className="size-4 shrink-0 text-destructive" aria-hidden="true" />
        {check.label}
      </span>
      <span className="mt-1 block break-all font-mono text-xs text-muted-foreground">
        {check.path}
      </span>
      <span className="mt-2 block text-xs text-muted-foreground">
        {check.reason}
      </span>
    </li>
  );
}

/**
 * The same status, as a settled panel rather than an interruption.
 *
 * Settings needs to answer "did I grant that?" long after the dialog is gone,
 * including when the answer is yes.
 */
export function SystemAccessSummary() {
  const access = useSystemAccess();
  const request = useRequestSystemAccess();

  if (access.data === undefined) return null;
  if (!access.data.gated) {
    return (
      <p className="text-sm text-muted-foreground">
        {access.data.platform === "windows" ? "Windows" : "Linux"} does not gate
        folder access, so there is nothing to grant here.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <ul className="flex flex-col gap-2">
        {access.data.checks.map((check) => (
          <li key={check.id} className="flex items-start gap-2 text-sm">
            <StateIcon state={check.state} />
            <span className="min-w-0">
              <span className="text-foreground">{check.label}</span>
              <span className="block break-all font-mono text-xs text-muted-foreground">
                {check.path}
              </span>
            </span>
          </li>
        ))}
      </ul>
      {access.data.needs_attention ? (
        <Button
          className="self-start"
          variant="outline"
          onClick={() => request.mutate()}
          disabled={request.isPending}
        >
          Grant access
        </Button>
      ) : null}
    </div>
  );
}

function StateIcon({ state }: { readonly state: AccessCheck["state"] }) {
  if (state === "denied") {
    return (
      <X
        className="mt-0.5 size-4 shrink-0 text-destructive"
        aria-label="Denied"
      />
    );
  }
  if (state === "missing") {
    return (
      <span
        className="mt-0.5 size-4 shrink-0 text-center text-muted-foreground"
        aria-label="Not found"
      >
        –
      </span>
    );
  }
  return (
    <Check
      className="mt-0.5 size-4 shrink-0 text-success"
      aria-label="Readable"
    />
  );
}
