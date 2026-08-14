/**
 * History calendar: list remains the default rail; month view lists
 * filed days from `list_standup_dates` instead of probing every date.
 */

import { expect, test } from "./support/fixtures";
import { makeScenario, makeStandupFile, TODAY } from "./support/scenario";

/**
 * The preview is headed by the standup file's own title line, not by a date the
 * UI formats itself — the file owns that string (`docs/specs/standup-file-format.md`).
 */
const STANDUP_TITLE = makeStandupFile().title;

test("history list still shows host counts from the standup cache @smoke", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/history");

  await expect(page.getByRole("heading", { name: "Last 14 days" })).toBeVisible();
  // One: the list rail ends at today (Aug 3), and the *other* seeded file is
  // tomorrow's — the one the dashboard is filling, which History cannot reach
  // until the calendar catches up.
  await expect(page.getByText("1 day with a standup file")).toBeVisible();
  await expect(page.getByRole("button", { name: /Aug 3/ })).toContainText("2 hosts");
  await expect(
    page.getByRole("heading", { name: STANDUP_TITLE }),
  ).toBeVisible();
});

test("history month view marks filed days and keeps the preview", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/history");

  await page.getByRole("tab", { name: "Month" }).click();

  await expect(page.getByRole("heading", { name: "August 2026" })).toBeVisible();
  await expect(app.callsTo("list_standup_dates")).resolves.toEqual(
    expect.arrayContaining([{ since: "2026-08-01", until: "2026-08-31" }]),
  );

  await page.getByRole("button", { name: TODAY }).click();
  await expect(
    page.getByRole("heading", { name: STANDUP_TITLE }),
  ).toBeVisible();
});
