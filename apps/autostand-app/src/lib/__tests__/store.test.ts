import { beforeEach, describe, expect, it } from "vitest";

import {
  SETTINGS_TABS,
  UI_STORE_KEY,
  isSettingsTab,
  useUiStore,
  type SettingsTab,
} from "@/lib/store";

// The store is a module singleton, so every test starts from a fresh copy of
// the initial state (actions included) instead of the previous test's leftovers.
const initialState = useUiStore.getState();

beforeEach(() => {
  useUiStore.setState(initialState, true);
});

describe("settingsTab", () => {
  it("opens Settings on Providers", () => {
    expect(useUiStore.getState().settingsTab).toBe("providers");
  });

  it("remembers the tab the user switched to", () => {
    useUiStore.getState().setSettingsTab("notifications");
    expect(useUiStore.getState().settingsTab).toBe("notifications");

    useUiStore.getState().setSettingsTab("local-models");
    expect(useUiStore.getState().settingsTab).toBe("local-models");
  });

  it("leaves the rest of the UI state alone", () => {
    const before = useUiStore.getState();
    useUiStore.getState().setSettingsTab("paths");
    const after = useUiStore.getState();

    expect(after.historyView).toBe(before.historyView);
    expect(after.selectedDate).toBe(before.selectedDate);
    expect(after.theme).toBe(before.theme);
  });
});

describe("SETTINGS_TABS", () => {
  it("lists every tab exactly once, in strip order", () => {
    expect(SETTINGS_TABS).toEqual([
      "providers",
      "data-sources",
      "format",
      "paths",
      "sync",
      "scheduler",
      "notifications",
      "local-models",
      "advanced",
    ]);
    expect(new Set(SETTINGS_TABS).size).toBe(SETTINGS_TABS.length);
  });

  it("starts on the default tab", () => {
    expect(SETTINGS_TABS[0]).toBe(useUiStore.getState().settingsTab);
  });
});

describe("isSettingsTab", () => {
  it("accepts every declared id", () => {
    for (const tab of SETTINGS_TABS) {
      expect(isSettingsTab(tab)).toBe(true);
    }
  });

  it("rejects anything else", () => {
    for (const value of [
      "Providers",
      "providers ",
      "data_sources",
      "history",
      "",
      null,
      undefined,
      0,
      {},
      ["providers"],
    ]) {
      expect(isSettingsTab(value)).toBe(false);
    }
  });

  it("narrows to SettingsTab for the caller", () => {
    const value: unknown = "scheduler";
    if (!isSettingsTab(value)) throw new Error("guard rejected a valid id");

    const narrowed: SettingsTab = value;
    expect(narrowed).toBe("scheduler");
  });
});

describe("sidebar gates", () => {
  it("hides Audit and Debug until someone asks for them", () => {
    expect(useUiStore.getState().showAuditNav).toBe(false);
    expect(useUiStore.getState().showDebugNav).toBe(false);
  });

  // They are separate switches: wanting the provenance sidecar says nothing
  // about wanting a gather preview.
  it("moves independently of each other", () => {
    useUiStore.getState().setShowAuditNav(true);
    expect(useUiStore.getState().showAuditNav).toBe(true);
    expect(useUiStore.getState().showDebugNav).toBe(false);

    useUiStore.getState().setShowDebugNav(true);
    useUiStore.getState().setShowAuditNav(false);
    expect(useUiStore.getState().showAuditNav).toBe(false);
    expect(useUiStore.getState().showDebugNav).toBe(true);
  });
});

describe("persistence", () => {
  // Read back through the store's own storage rather than reaching for
  // localStorage: the store falls back to memory wherever web storage is
  // unavailable, and the test has to follow it there.
  function persistedState(): Record<string, unknown> {
    const storage = useUiStore.persist.getOptions().storage;
    if (storage === undefined) throw new Error("the store has no storage");
    const entry = storage.getItem(UI_STORE_KEY);
    if (entry === null || entry instanceof Promise) {
      throw new Error("the store persisted nothing");
    }
    return entry.state as Record<string, unknown>;
  }

  // A preference the user sets by hand is worthless if it resets on restart.
  it("keeps the theme and both sidebar gates", () => {
    useUiStore.getState().setTheme("dark");
    useUiStore.getState().setShowAuditNav(true);

    expect(persistedState()).toEqual({
      theme: "dark",
      showAuditNav: true,
      showDebugNav: false,
    });
  });

  // Restoring a focused date would reopen the app pointing at a day that is no
  // longer today, and a restored terminal panel would reopen over the dashboard.
  it("leaves session state out", () => {
    useUiStore.getState().setSelectedDate("1999-12-31");
    useUiStore.getState().setTerminalPanel("open");
    useUiStore.getState().setHistoryView("month");
    useUiStore.getState().setSidebarCollapsed(true);

    const keys = Object.keys(persistedState()).sort();
    expect(keys).toEqual(["showAuditNav", "showDebugNav", "theme"]);
  });
});
