/**
 * Sidebar visibility of the diagnostic routes.
 *
 * The rule: hiding Audit or Debug removes it from the rail and nothing else.
 * Their routes stay registered, so a bookmark or the browser history still
 * reaches them — this is about what a new user is asked to look at, not about
 * taking a feature away.
 */

import { describe, expect, it } from "vitest";

import { NAV_ITEMS, visibleNavItems } from "@/lib/nav";

const labels = (gates: { showAuditNav: boolean; showDebugNav: boolean }) =>
  visibleNavItems(gates).map((item) => item.label);

describe("visibleNavItems", () => {
  it("hides Audit and Debug by default", () => {
    expect(labels({ showAuditNav: false, showDebugNav: false })).toEqual([
      "Dashboard",
      "History",
      "Settings",
    ]);
  });

  // Each gate stands alone: wanting the provenance sidecar says nothing about
  // wanting a gather preview.
  it("reveals Audit without dragging Debug along", () => {
    const shown = labels({ showAuditNav: true, showDebugNav: false });
    expect(shown).toContain("Audit");
    expect(shown).not.toContain("Debug");
  });

  it("reveals Debug without dragging Audit along", () => {
    const shown = labels({ showAuditNav: false, showDebugNav: true });
    expect(shown).toContain("Debug");
    expect(shown).not.toContain("Audit");
  });

  it("keeps declaration order with both on", () => {
    expect(labels({ showAuditNav: true, showDebugNav: true })).toEqual([
      "Dashboard",
      "History",
      "Audit",
      "Debug",
      "Settings",
    ]);
  });

  // A new gated entry must be opt-in, or adding one silently ships it to
  // everyone the moment it lands.
  it("gates nothing beyond Audit and Debug", () => {
    const gated = NAV_ITEMS.filter((item) => item.gate !== undefined);
    expect(gated.map((item) => item.to)).toEqual(["/audit", "/debug"]);
  });
});
