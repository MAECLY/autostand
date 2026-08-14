import { useEffect } from "react";

import { createRootRoute, Outlet } from "@tanstack/react-router";

import { Sidebar } from "@/components/layout/Sidebar";
import { StatusBar } from "@/components/layout/StatusBar";
import { TopBar } from "@/components/layout/TopBar";
import { TerminalPanel } from "@/components/standup/TerminalPanel";
import { usePipelineEvents } from "@/hooks/use-pipeline-status";
import { useProviderHealthEvents } from "@/hooks/use-providers";
import { applyTheme, useUiStore } from "@/lib/store";

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout() {
  usePipelineEvents();
  useProviderHealthEvents();
  useAppliedTheme();
  const panel = useUiStore((s) => s.terminalPanel);
  const panelHeight = useUiStore((s) => s.terminalPanelHeight);

  const gridRows =
    panel === "open"
      ? `auto minmax(0,1fr) ${panelHeight}px auto`
      : panel === "minimized"
        ? "auto minmax(0,1fr) 2rem auto"
        : "auto minmax(0,1fr) auto";

  return (
    <div className="grid h-full grid-cols-[auto_1fr] overflow-hidden bg-background text-foreground">
      <Sidebar />
      <div
        className="grid min-h-0 min-w-0 overflow-hidden"
        style={{ gridTemplateRows: gridRows }}
      >
        <TopBar />
        <main className="min-h-0 overflow-y-auto">
          <Outlet />
        </main>
        <TerminalPanel />
        <StatusBar />
      </div>
    </div>
  );
}

/** Mirror the stored theme onto `<html>`, following the OS while it is "system". */
function useAppliedTheme(): void {
  const theme = useUiStore((state) => state.theme);

  useEffect(() => {
    applyTheme(theme);
    if (theme !== "system") return;

    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => applyTheme("system");
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, [theme]);
}
