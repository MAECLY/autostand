import { describe, expect, it } from "vitest";

import {
  daysInRange,
  historyRange,
  historyRangeLabel,
  monthGridDays,
  shiftHistoryAnchor,
  toFilingDate,
} from "./range";

describe("historyRange", () => {
  it("lists the 14 days ending on the anchor", () => {
    expect(historyRange("list", "2026-08-03")).toEqual({
      since: "2026-07-21",
      until: "2026-08-03",
    });
  });

  it("uses the calendar month for month and agenda", () => {
    expect(historyRange("month", "2026-08-03")).toEqual({
      since: "2026-08-01",
      until: "2026-08-31",
    });
    expect(historyRange("agenda", "2026-08-03")).toEqual({
      since: "2026-08-01",
      until: "2026-08-31",
    });
  });

  it("uses a Monday-starting week", () => {
    expect(historyRange("week", "2026-08-03")).toEqual({
      since: "2026-08-03",
      until: "2026-08-09",
    });
  });

  it("pins day view to the anchor", () => {
    expect(historyRange("day", "2026-08-03")).toEqual({
      since: "2026-08-03",
      until: "2026-08-03",
    });
  });
});

describe("shiftHistoryAnchor", () => {
  it("moves list windows by a fortnight", () => {
    expect(shiftHistoryAnchor("list", "2026-08-03", -1)).toBe("2026-07-20");
    expect(shiftHistoryAnchor("list", "2026-08-03", 1)).toBe("2026-08-17");
  });

  it("moves month and agenda by a calendar month", () => {
    expect(shiftHistoryAnchor("month", "2026-08-03", -1)).toBe("2026-07-03");
    expect(shiftHistoryAnchor("agenda", "2026-01-31", 1)).toBe("2026-02-28");
  });
});

describe("historyRangeLabel", () => {
  it("keeps the list caption the e2e spec already pins", () => {
    expect(historyRangeLabel("list", "2026-08-03")).toBe("Last 14 days");
  });

  it("names the month", () => {
    expect(historyRangeLabel("month", "2026-08-03")).toBe("August 2026");
  });
});

describe("daysInRange", () => {
  it("includes both ends", () => {
    expect(daysInRange("2026-08-01", "2026-08-03")).toEqual([
      "2026-08-01",
      "2026-08-02",
      "2026-08-03",
    ]);
  });
});

describe("monthGridDays", () => {
  it("pads August 2026 from Monday Jul 27 to Sunday Sep 6", () => {
    const days = monthGridDays("2026-08-03");
    expect(days).toHaveLength(42);
    expect(toFilingDate(days[0]!)).toBe("2026-07-27");
    expect(toFilingDate(days[days.length - 1]!)).toBe("2026-09-06");
  });
});
