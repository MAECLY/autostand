/**
 * The sidebar's entries and the rule that decides which of them show.
 *
 * Separate from `Sidebar.tsx` so the rule can be tested directly: `<Link>` needs
 * a full router context, which would add setup without adding any signal about
 * which entries appear.
 */

import {
  Bug,
  History,
  LayoutDashboard,
  Settings,
  ShieldCheck,
  type LucideIcon,
} from "lucide-react";

import type { UiState } from "@/lib/store";

export interface NavItem {
  /** Must stay a literal union so TanStack Router can type-check the link. */
  readonly to: "/" | "/history" | "/audit" | "/debug" | "/settings";
  readonly label: string;
  readonly icon: LucideIcon;
  /**
   * UI-store flag that gates this entry, when one does.
   *
   * The route stays registered either way: a bookmark, a deep link or the
   * browser history still reaches it. This only decides whether someone who has
   * never heard of a provenance sidecar has to look at one.
   */
  readonly gate?: "showAuditNav" | "showDebugNav";
}

export const NAV_ITEMS: readonly NavItem[] = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard },
  { to: "/history", label: "History", icon: History },
  { to: "/audit", label: "Audit", icon: ShieldCheck, gate: "showAuditNav" },
  { to: "/debug", label: "Debug", icon: Bug, gate: "showDebugNav" },
  { to: "/settings", label: "Settings", icon: Settings },
];

/** The rail's entries for a given set of gates, in declaration order. */
export function visibleNavItems(
  gates: Pick<UiState, "showAuditNav" | "showDebugNav">,
): readonly NavItem[] {
  return NAV_ITEMS.filter((item) =>
    item.gate === undefined ? true : gates[item.gate],
  );
}
