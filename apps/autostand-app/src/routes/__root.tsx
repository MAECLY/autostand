import { useEffect } from "react";

import { createRootRoute, Outlet } from "@tanstack/react-router";

import { Sidebar } from "@/components/layout/Sidebar";
import { StatusBar } from "@/components/layout/StatusBar";
import { TopBar } from "@/components/layout/TopBar";
import { usePipelineEvents } from "@/hooks/use-pipeline-status";
import { applyTheme, useUiStore } from "@/lib/store";

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout() {
  // Mounted once for the whole app: the backend event stream feeds the cache.
  usePipelineEvents();
  useAppliedTheme();

  return (
    <div className="grid h-full grid-cols-[auto_1fr] overflow-hidden bg-background text-foreground">
      <Sidebar />
      <div className="grid min-w-0 grid-rows-[auto_minmax(0,1fr)_auto]">
        <TopBar />
        {/* The only scroll container in the shell. */}
        <main className="min-h-0 overflow-y-auto">
          <Outlet />
        </main>
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
