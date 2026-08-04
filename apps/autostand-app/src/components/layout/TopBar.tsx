/**
 * Top chrome: page title, sidebar collapse, theme switch, and a right-hand slot
 * pages fill with their primary action (e.g. "Compile now").
 */

import type { ReactNode } from "react";

import { useRouterState } from "@tanstack/react-router";
import {
  Monitor,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  Sun,
  type LucideIcon,
} from "lucide-react";

import { Button } from "@autostand/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@autostand/ui/components/dropdown-menu";

import { useUiStore, type Theme } from "@/lib/store";

/**
 * Titles are keyed by route id (not pathname) so a renamed URL cannot silently
 * fall through to the brand name. Kept local: the page routes are owned
 * elsewhere and expose no static title metadata.
 */
const ROUTE_TITLES: Record<string, string | undefined> = {
  "/": "Dashboard",
  "/history": "History",
  "/audit": "Audit",
  "/debug": "Debug",
  "/settings": "Settings",
};

const THEME_OPTIONS: readonly { value: Theme; label: string; icon: LucideIcon }[] = [
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
  { value: "system", label: "System", icon: Monitor },
];

/** Narrow the menu's raw string value without casting. */
function toTheme(value: string): Theme | null {
  return THEME_OPTIONS.find((option) => option.value === value)?.value ?? null;
}

export interface TopBarProps {
  /** Page-supplied actions, rendered right of the title. */
  children?: ReactNode;
}

export function TopBar({ children }: TopBarProps) {
  const collapsed = useUiStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useUiStore((state) => state.toggleSidebar);
  const theme = useUiStore((state) => state.theme);
  const setTheme = useUiStore((state) => state.setTheme);

  const title = useRouterState({
    select: (state) => {
      const leaf = state.matches[state.matches.length - 1];
      return (leaf && ROUTE_TITLES[leaf.routeId]) ?? "autostand";
    },
  });

  const ThemeIcon =
    THEME_OPTIONS.find((option) => option.value === theme)?.icon ?? Monitor;

  return (
    <header className="flex h-14 shrink-0 items-center gap-3 border-b border-border bg-surface px-4">
      <Button
        variant="ghost"
        size="icon"
        onClick={toggleSidebar}
        aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      >
        {collapsed ? (
          <PanelLeftOpen className="size-4" aria-hidden />
        ) : (
          <PanelLeftClose className="size-4" aria-hidden />
        )}
      </Button>

      <h1 className="truncate text-base font-semibold">{title}</h1>

      <div className="ml-auto flex items-center gap-2">
        {children}

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" aria-label="Change theme">
              <ThemeIcon className="size-4" aria-hidden />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuRadioGroup
              value={theme}
              onValueChange={(value) => {
                const next = toTheme(value);
                if (next) setTheme(next);
              }}
            >
              {THEME_OPTIONS.map((option) => (
                <DropdownMenuRadioItem key={option.value} value={option.value}>
                  <option.icon className="size-4" aria-hidden />
                  {option.label}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </header>
  );
}
