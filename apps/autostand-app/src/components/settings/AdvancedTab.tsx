/**
 * Settings → Advanced: surfaces that are useful once you know what they mean.
 *
 * Nothing here changes what the app *does* — only what a user is asked to look
 * at. Anything that alters pipeline behaviour belongs in its own tab.
 */

import { useEffect, useState } from "react";

import { UpdatesCard } from "@/components/settings/UpdatesCard";
import { tauriUpdater } from "@/lib/tauri";
import { Label } from "@autostand/ui/components/label";
import { Switch } from "@autostand/ui/components/switch";

import { useUiStore } from "@/lib/store";

interface NavToggleProps {
  id: string;
  label: string;
  description: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}

function NavToggle({
  id,
  label,
  description,
  checked,
  onCheckedChange,
}: NavToggleProps) {
  return (
    <div className="flex items-center justify-between gap-6 border-b border-border py-4 last:border-b-0">
      <div className="min-w-0">
        <Label htmlFor={id}>{label}</Label>
        <p className="text-xs text-muted-foreground">{description}</p>
      </div>
      <Switch id={id} checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  );
}

export function AdvancedTab() {
  // Read once at module scope by the plugin; a query would be a round trip for
  // a value that cannot change while the process lives.
  const [version, setVersion] = useState("…");
  useEffect(() => {
    void tauriUpdater.currentVersion().then(setVersion);
  }, []);

  const showAuditNav = useUiStore((state) => state.showAuditNav);
  const showDebugNav = useUiStore((state) => state.showDebugNav);
  const setShowAuditNav = useUiStore((state) => state.setShowAuditNav);
  const setShowDebugNav = useUiStore((state) => state.setShowDebugNav);

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h2 className="text-sm font-semibold">Updates</h2>
        <p className="text-xs text-muted-foreground">
          Install a new version from here instead of downloading an installer
          again.
        </p>
      </div>

      <UpdatesCard currentVersion={version} />

      <div>
        <h2 className="text-sm font-semibold">Sidebar</h2>
        <p className="text-xs text-muted-foreground">
          Diagnostic screens, hidden by default. Both keep working while hidden
          — <code>/audit</code> and <code>/debug</code> stay reachable by URL.
        </p>
      </div>

      <div className="rounded-xl border border-border bg-surface px-4">
        <NavToggle
          id="show-audit-nav"
          label="Show Audit"
          description="The provenance sidecar behind each standup: which facts, sources and provider produced it."
          checked={showAuditNav}
          onCheckedChange={setShowAuditNav}
        />
        <NavToggle
          id="show-debug-nav"
          label="Show Debug"
          description="A preview of what the data sources gathered, before anything is rendered."
          checked={showDebugNav}
          onCheckedChange={setShowDebugNav}
        />
      </div>
    </div>
  );
}
