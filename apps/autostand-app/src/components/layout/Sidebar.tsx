/**
 * Left navigation rail. Width is driven by `sidebarCollapsed` in the UI store;
 * collapsed mode drops the labels and moves them into tooltips.
 */

import { Link } from "@tanstack/react-router";
import {
  Bug,
  Check,
  History,
  LayoutDashboard,
  Settings,
  ShieldCheck,
  type LucideIcon,
} from "lucide-react";

import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@autostand/ui/components/tooltip";

import { useUiStore } from "@/lib/store";
import { cn } from "@/lib/utils";

interface NavItem {
  /** Must stay a literal union so TanStack Router can type-check the link. */
  readonly to: "/" | "/history" | "/audit" | "/debug" | "/settings";
  readonly label: string;
  readonly icon: LucideIcon;
}

const NAV_ITEMS: readonly NavItem[] = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard },
  { to: "/history", label: "History", icon: History },
  { to: "/audit", label: "Audit", icon: ShieldCheck },
  { to: "/debug", label: "Debug", icon: Bug },
  { to: "/settings", label: "Settings", icon: Settings },
];

const ITEM_BASE =
  "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

const ITEM_ACTIVE = "bg-muted text-primary hover:text-primary";

export function Sidebar() {
  const collapsed = useUiStore((state) => state.sidebarCollapsed);

  return (
    <aside
      className={cn(
        "flex h-full flex-col border-r border-border bg-surface transition-[width] duration-200",
        collapsed ? "w-16" : "w-56",
      )}
    >
      <div
        className={cn(
          "flex h-14 shrink-0 items-center gap-2 border-b border-border",
          collapsed ? "justify-center px-0" : "px-4",
        )}
      >
        {/* The brand mark: a standup card with a completion check. */}
        <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary">
          <Check
            className="size-5 text-primary-foreground"
            strokeWidth={3}
            aria-hidden
          />
        </span>
        {!collapsed && (
          <span className="font-sans text-lg font-bold tracking-tight">
            autostand
          </span>
        )}
      </div>

      <TooltipProvider delayDuration={0}>
        <nav
          aria-label="Main"
          className={cn("flex flex-1 flex-col gap-1 py-3", collapsed ? "px-2" : "px-3")}
        >
          {NAV_ITEMS.map((item) => {
            const link = (
              <Link
                key={item.to}
                to={item.to}
                activeOptions={{ exact: true }}
                activeProps={{ className: ITEM_ACTIVE }}
                className={cn(ITEM_BASE, collapsed && "justify-center px-0")}
              >
                <item.icon className="size-4 shrink-0" aria-hidden />
                {collapsed ? (
                  <span className="sr-only">{item.label}</span>
                ) : (
                  <span className="truncate">{item.label}</span>
                )}
              </Link>
            );

            if (!collapsed) return link;

            return (
              <Tooltip key={item.to}>
                <TooltipTrigger asChild>{link}</TooltipTrigger>
                <TooltipContent side="right">{item.label}</TooltipContent>
              </Tooltip>
            );
          })}
        </nav>
      </TooltipProvider>
    </aside>
  );
}
