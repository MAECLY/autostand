import { format } from "date-fns";
import { describe, expect, it } from "vitest";

import {
  ISO_DATE_FORMAT,
  cn,
  formatIsoDate,
  formatRelative,
  hostColor,
  todayIso,
} from "@/lib/utils";

describe("cn", () => {
  it("keeps the last of two conflicting Tailwind utilities", () => {
    expect(cn("px-2", "px-4")).toBe("px-4");
    expect(cn("text-sm text-muted-foreground", "text-foreground")).toBe(
      "text-sm text-foreground",
    );
  });

  it("keeps utilities that do not conflict", () => {
    expect(cn("rounded-md", "border-border")).toBe("rounded-md border-border");
  });

  it("drops falsy values and flattens conditional shapes", () => {
    expect(cn("font-mono", false, null, undefined, ["shadow-sm"])).toBe(
      "font-mono shadow-sm",
    );
    expect(cn({ "bg-inset": true, "bg-elevated": false })).toBe("bg-inset");
  });

  it("returns an empty string when nothing is passed", () => {
    expect(cn()).toBe("");
  });
});

describe("formatIsoDate", () => {
  it("formats a filing date for display", () => {
    expect(formatIsoDate("2026-08-03")).toBe("Aug 3, 2026");
  });

  it("formats a full timestamp", () => {
    expect(formatIsoDate("2026-08-03T07:15:00")).toBe("Aug 3, 2026");
  });

  it("honours a caller-supplied pattern", () => {
    expect(formatIsoDate("2026-08-03", ISO_DATE_FORMAT)).toBe("2026-08-03");
  });

  it("returns unparseable input verbatim instead of 'Invalid Date'", () => {
    expect(formatIsoDate("not-a-date")).toBe("not-a-date");
    expect(formatIsoDate("")).toBe("");
  });
});

describe("formatRelative", () => {
  it("describes a past instant relative to now", () => {
    const twoHoursAgo = new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString();
    expect(formatRelative(twoHoursAgo)).toBe("about 2 hours ago");
  });

  it("returns unparseable input verbatim", () => {
    expect(formatRelative("never")).toBe("never");
  });
});

describe("todayIso", () => {
  it("returns today in the backend filing-date format", () => {
    expect(todayIso()).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(todayIso()).toBe(format(new Date(), ISO_DATE_FORMAT));
  });
});

describe("hostColor", () => {
  const PALETTE = [
    "text-audit-commit",
    "text-audit-github",
    "text-audit-review",
    "text-audit-note",
    "text-audit-phantom",
  ];

  const SLUGS = [
    "mbp-miguel",
    "linux-lab",
    "win-desk",
    "mac-mini",
    "pi-node",
    "ci-runner",
    "a",
  ];

  it("returns the same class every time for the same slug", () => {
    for (const slug of SLUGS) {
      expect(hostColor(slug)).toBe(hostColor(slug));
    }
    // Stability is the contract: the value must not depend on call order.
    expect(hostColor("mbp-miguel")).toBe(hostColor("mbp-miguel"));
  });

  it("only ever returns a palette token", () => {
    for (const slug of SLUGS) {
      expect(PALETTE).toContain(hostColor(slug));
    }
  });

  it("spreads distinct slugs across more than one hue", () => {
    const distinct = new Set(SLUGS.map(hostColor));
    expect(distinct.size).toBeGreaterThan(1);
  });

  it("falls back to the unverified token for an empty slug", () => {
    expect(hostColor("")).toBe("text-audit-unverified");
  });
});
