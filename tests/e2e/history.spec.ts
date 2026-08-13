/**
 * History calendar: list remains the default rail; month view lists
 * filed days from `list_standup_dates` instead of probing every date.
 */

import { expect, test } from "./support/fixtures";
import { makeScenario, TODAY, TODAY_LABEL } from "./support/scenario";

test("history list still shows host counts from the standup cache @smoke", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/history");

  await expect(page.getByRole("heading", { name: "Last 14 days" })).toBeVisible();
  await expect(page.getByText("1 day with a standup file")).toBeVisible();
  await expect(page.getByRole("button", { name: /Aug 3/ })).toContainText("2 hosts");
  await expect(
    page.getByRole("heading", { name: `Daily Standup — ${TODAY_LABEL}` }),
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
    page.getByRole("heading", { name: `Daily Standup — ${TODAY_LABEL}` }),
  ).toBeVisible();
});
