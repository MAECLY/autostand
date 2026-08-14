/**
 * Pure UI state. Anything that comes from the backend lives in TanStack Query,
 * never here — this store only holds what the user toggles in the shell.
 */

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

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

export type HistoryView = "list" | "month" | "week" | "day" | "agenda";

/** Settings tab ids in strip order; `routes/settings.tsx` renders from this. */
export const SETTINGS_TABS = [
  "providers",
  "data-sources",
  "format",
  "paths",
  "sync",
  "scheduler",
  "notifications",
  "local-models",
  "advanced",
] as const;

export type SettingsTab = (typeof SETTINGS_TABS)[number];

/** Radix hands tab changes back as a bare string; narrow before storing. */
export function isSettingsTab(value: unknown): value is SettingsTab {
  return (SETTINGS_TABS as readonly unknown[]).includes(value);
}

export interface UiState {
  theme: Theme;
  sidebarCollapsed: boolean;
  /** Filing date the UI is focused on, `YYYY-MM-DD`. */
  selectedDate: string;
  /** Visible History range is built around this filing date. */
  historyAnchor: string;
  historyView: HistoryView;
  /** Settings tab to reopen on: the route unmounts on every navigation away. */
  settingsTab: SettingsTab;
  /**
   * Whether Audit appears in the sidebar.
   *
   * Off by default: it shows the provenance sidecar behind a standup, which
   * only means something once you know what one is. The `/audit` route stays
   * registered either way — this hides the rail entry, it does not remove the
   * feature.
   */
  showAuditNav: boolean;
  /** Whether Debug appears in the sidebar. Same contract as [`showAuditNav`]. */
  showDebugNav: boolean;
  /** VSCode-style bottom panel state for the pipeline log viewer. */
  terminalPanel: TerminalPanelState;
  /** Bottom panel height in px when open. */
  terminalPanelHeight: number;
  setTheme: (theme: Theme) => void;
  toggleSidebar: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  setSelectedDate: (date: string) => void;
  setHistoryAnchor: (date: string) => void;
  setHistoryView: (view: HistoryView) => void;
  setSettingsTab: (tab: SettingsTab) => void;
  setShowAuditNav: (show: boolean) => void;
  setShowDebugNav: (show: boolean) => void;
  setTerminalPanel: (state: TerminalPanelState) => void;
  setTerminalPanelHeight: (height: number) => void;
}

const DEFAULT_PANEL_HEIGHT = 240;

/** Storage key for the slice of this store that outlives a session. */
export const UI_STORE_KEY = "autostand-ui";

/**
 * Web storage, or an in-memory stand-in when there is none.
 *
 * `localStorage` is absent in a bare Node context and can throw outright when a
 * webview runs with site data disabled. Losing a preference is a papercut;
 * taking the whole shell down over one is not, so the store degrades to
 * session-only rather than failing to construct.
 */
function preferenceStorage(): Storage {
  try {
    if (typeof globalThis.localStorage !== "undefined") {
      return globalThis.localStorage;
    }
  } catch {
    // Access itself throws when storage is blocked; fall through.
  }
  const memory = new Map<string, string>();
  return {
    get length() {
      return memory.size;
    },
    clear: () => memory.clear(),
    getItem: (key) => memory.get(key) ?? null,
    key: (index) => [...memory.keys()][index] ?? null,
    removeItem: (key) => void memory.delete(key),
    setItem: (key, value) => void memory.set(key, value),
  };
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      theme: "system",
      sidebarCollapsed: false,
      selectedDate: todayIso(),
      historyAnchor: todayIso(),
      historyView: "list",
      settingsTab: "providers",
      showAuditNav: false,
      showDebugNav: false,
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
      setHistoryAnchor: (historyAnchor) => set({ historyAnchor }),
      setHistoryView: (historyView) => set({ historyView }),
      setSettingsTab: (settingsTab) => set({ settingsTab }),
      setShowAuditNav: (showAuditNav) => set({ showAuditNav }),
      setShowDebugNav: (showDebugNav) => set({ showDebugNav }),
      setTerminalPanel: (terminalPanel) => set({ terminalPanel }),
      setTerminalPanelHeight: (terminalPanelHeight) =>
        set({ terminalPanelHeight }),
    }),
    {
      name: UI_STORE_KEY,
      storage: createJSONStorage(preferenceStorage),
      // Only what the user deliberately chose. The focused date, the history
      // anchor and the terminal panel are session state: restoring them would
      // reopen the app pointing at a day that is no longer today.
      partialize: (state) => ({
        theme: state.theme,
        showAuditNav: state.showAuditNav,
        showDebugNav: state.showDebugNav,
      }),
      // The stored theme has to reach `<html>` as soon as it is read, or the
      // app paints in the default and flips a frame later.
      onRehydrateStorage: () => (state) => {
        if (state !== undefined) applyTheme(state.theme);
      },
    },
  ),
);
