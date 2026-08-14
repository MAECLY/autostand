import { describe, expect, it } from "vitest";

import {
  cronFromHumanSchedule,
  parseHumanSchedule,
  scheduleDescription,
} from "@/components/settings/SchedulerForm";

describe("human schedule builder", () => {
  it("round-trips the default hourly workday schedule", () => {
    const parsed = parseHumanSchedule("0 7-19 * * 1-5");
    expect(parsed).not.toBeNull();
    expect(cronFromHumanSchedule(parsed!)).toBe("0 7-19 * * 1-5");
    expect(scheduleDescription(parsed!)).toContain("Monday through Friday");
  });

  it("expresses a one-time schedule without requiring cron knowledge", () => {
    expect(
      cronFromHumanSchedule({
        kind: "once",
        minute: 30,
        hour: 9,
        endHour: 17,
        days: [1, 3, 5],
      }),
    ).toBe("30 9 * * 1,3,5");
  });

  it("leaves unsupported cron available to the advanced editor", () => {
    expect(parseHumanSchedule("*/15 7-19 * * 1-5")).toBeNull();
  });
});
