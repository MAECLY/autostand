/**
 * The header has one job: never let the work day and the destination file be
 * mistaken for each other. Every assertion below is about that distinction.
 */

import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";

import {
  StandupTargetHeader,
  relationLabel,
  windowLabel,
} from "@/components/standup/StandupTargetHeader";
import { makeFilingTarget } from "@/test/mocks";
import { renderWithProviders } from "@/test/render";

describe("StandupTargetHeader", () => {
  it("names the work day and the file as two different dates", () => {
    renderWithProviders(<StandupTargetHeader target={makeFilingTarget()} />);

    expect(
      screen.getByRole("heading", { name: "Today's work — Aug 3, 2026" }),
    ).toBeDefined();
    // Verbatim, so a user can search the dailies directory for it.
    expect(screen.getByText("2026-08-04.md")).toBeDefined();
  });

  it("says which standup the file is, without guessing a weekday", () => {
    // Friday's work files into Monday, so "tomorrow's standup" would be wrong
    // once a week — the header says what is always true instead.
    const friday = makeFilingTarget({
      work_day: "2026-08-07",
      filing_date: "2026-08-10",
      window: { range_start: "2026-08-07", range_end: "2026-08-07" },
    });
    renderWithProviders(<StandupTargetHeader target={friday} />);

    expect(screen.getByText(/the next business day's standup/)).toBeDefined();
  });

  it("says today's standup when the policy files same-day", () => {
    const sameDay = makeFilingTarget({
      filing_date: "2026-08-03",
      archive_mode: "same_day",
    });
    renderWithProviders(<StandupTargetHeader target={sameDay} />);

    expect(screen.getByText(/today's standup/)).toBeDefined();
    expect(screen.getByText("2026-08-03.md")).toBeDefined();
  });

  it("shows the window a compile would claim", () => {
    const weekend = makeFilingTarget({
      work_day: "2026-08-10",
      filing_date: "2026-08-11",
      window: { range_start: "2026-08-07", range_end: "2026-08-09" },
    });
    renderWithProviders(<StandupTargetHeader target={weekend} />);

    expect(screen.getByText(/Covers Aug 7, 2026 – Aug 9, 2026\./)).toBeDefined();
  });

  it("says nothing is claimable rather than printing an inverted range", () => {
    const ahead = makeFilingTarget({
      window: { range_start: "2026-08-04", range_end: "2026-08-03" },
      window_empty: true,
    });
    renderWithProviders(<StandupTargetHeader target={ahead} />);

    expect(
      screen.getByText("Nothing to file yet: 2026-08-04 is ahead of today."),
    ).toBeDefined();
    expect(screen.queryByText(/^Covers/)).toBeNull();
  });
});

describe("windowLabel", () => {
  it("collapses a single-day window to one date", () => {
    expect(windowLabel("2026-08-03", "2026-08-03")).toBe("Aug 3, 2026");
  });

  it("prints a range with an en dash", () => {
    expect(windowLabel("2026-08-07", "2026-08-09")).toBe(
      "Aug 7, 2026 – Aug 9, 2026",
    );
  });
});

describe("relationLabel", () => {
  it("depends on the dates, not on the configured mode name", () => {
    // A same-day *policy* still files a weekend forward, so the label has to
    // read the resolved dates rather than `archive_mode`.
    const weekendUnderSameDay = makeFilingTarget({
      work_day: "2026-08-08",
      filing_date: "2026-08-10",
      archive_mode: "same_day",
    });
    expect(relationLabel(weekendUnderSameDay)).toBe(
      "the next business day's standup",
    );
  });
});
