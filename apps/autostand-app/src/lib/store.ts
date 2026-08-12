/**
 * Pure UI state. Anything that comes from the backend lives in TanStack Query,
 * never here — this store only holds what the user toggles in the shell.
 */

import { create } from "zustand";

import { todayIso } from "@/lib/utils";

export type Theme = "light" | "dark" | "system";

function prefersDark(): boolean {
  return (
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches
  );
}

/** Reflect `theme` onto `<html class="dark">`; the tokens flip off that class. */
export function applyTheme(theme: Theme): void {
  if (typeof document === "undefined") return;
  const dark = theme === "dark" || (theme === "system" && prefersDark());
  document.documentElement.classList.toggle("dark", dark);
}

export type TerminalPanelState = "open" | "closed" | "minimized";

export interface UiState {
  theme: Theme;
  sidebarCollapsed: boolean;
  /** Filing date the UI is focused on, `YYYY-MM-DD`. */
  selectedDate: string;
  /** VSCode-style bottom panel state for the pipeline log viewer. */
  terminalPanel: TerminalPanelState;
  /** Bottom panel height in px when open. */
  terminalPanelHeight: number;
  setTheme: (theme: Theme) => void;
  toggleSidebar: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  setSelectedDate: (date: string) => void;
  setTerminalPanel: (state: TerminalPanelState) => void;
  setTerminalPanelHeight: (height: number) => void;
}

const DEFAULT_PANEL_HEIGHT = 240;

export const useUiStore = create<UiState>()((set) => ({
  theme: "system",
  sidebarCollapsed: false,
  selectedDate: todayIso(),
  terminalPanel: "closed",
  terminalPanelHeight: DEFAULT_PANEL_HEIGHT,
  setTheme: (theme) => {
    applyTheme(theme);
    set({ theme });
  },
  toggleSidebar: () =>
    set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
  setSidebarCollapsed: (sidebarCollapsed) => set({ sidebarCollapsed }),
  setSelectedDate: (selectedDate) => set({ selectedDate }),
  setTerminalPanel: (terminalPanel) => set({ terminalPanel }),
  setTerminalPanelHeight: (terminalPanelHeight) => set({ terminalPanelHeight }),
}));
