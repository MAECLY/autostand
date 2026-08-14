import { beforeEach, describe, expect, it } from "vitest";

import {
  SETTINGS_TABS,
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
