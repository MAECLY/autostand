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

export interface UiState {
  theme: Theme;
  sidebarCollapsed: boolean;
  /** Filing date the UI is focused on, `YYYY-MM-DD`. */
  selectedDate: string;
  setTheme: (theme: Theme) => void;
  toggleSidebar: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  setSelectedDate: (date: string) => void;
}

export const useUiStore = create<UiState>()((set) => ({
  theme: "system",
  sidebarCollapsed: false,
  selectedDate: todayIso(),
  setTheme: (theme) => {
    applyTheme(theme);
    set({ theme });
  },
  toggleSidebar: () =>
    set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
  setSidebarCollapsed: (sidebarCollapsed) => set({ sidebarCollapsed }),
  setSelectedDate: (selectedDate) => set({ selectedDate }),
}));
