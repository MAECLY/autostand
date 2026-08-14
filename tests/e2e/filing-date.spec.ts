/**
 * The filing-date policy, from the Settings control to the file the dashboard
 * announces.
 *
 * This is the journey the bug lived in. Autostand labelled the dashboard with
 * the calendar day and compiled that same day, so a machine ended up with a
 * `2026-08-13.md` and no `2026-08-14.md` — and nothing on screen said which file
 * "Compile now" was about to write. The round trip below is the proof that the
 * setting and the announcement are the same fact: change the policy in Settings,
 * and the dashboard names a different file and asks the backend for it.
 *
 * The mock backend derives the target with the same rule as
 * `autostand_core::dates`, so a spec cannot assert a target the real one would
 * never produce (`support/mock-backend.ts` § filing dates).
 */

import { expect, test } from "./support/fixtures";
import {
  FILING_DATE,
  makeFilingDateStandupFile,
  makeScenario,
  makeStandupFile,
  TODAY,
  TODAY_LABEL,
} from "./support/scenario";

/** Settings opens on Providers; the filing-date card lives on Paths. */
async function openPaths(page: import("@playwright/test").Page) {
  await page.getByRole("link", { name: "Settings" }).click();
  await page.getByRole("tab", { name: "Paths" }).click();
  await expect(page.getByRole("heading", { name: "Filing date" })).toBeVisible();
}

test("states the consequence of each policy, not its internal name @smoke", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/settings");
  await page.getByRole("tab", { name: "Paths" }).click();

  const card = page.getByRole("radiogroup", { name: "Filing date" });
  await expect(
    card.getByText("Today's work is filed for tomorrow's standup."),
  ).toBeVisible();
  await expect(
    card.getByText("Today's work is filed for today's standup."),
  ).toBeVisible();

  // The default is the App Script's rule, and it is labelled as such.
  await expect(
    card.getByRole("radio", { name: /Next business day/ }),
  ).toHaveAttribute("aria-checked", "true");
  await expect(card.getByText("Original")).toBeVisible();

  // The rule neither option changes has to be stated once, or a user picking
  // "Same day" will expect a Saturday file that never exists.
  await expect(
    page.getByText(
      "Either way, weekend work accumulates into Monday's file",
      { exact: false },
    ),
  ).toBeVisible();

  // And the card says what the choice means today, in file names.
  await expect(
    page.getByText(`is filed in ${FILING_DATE}.md`, { exact: false }),
  ).toBeVisible();
});

test("changing the policy changes the file the dashboard announces", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  await app.start(scenario, "/");

  // Before: Monday's work files into Tuesday.
  await expect(
    page.getByRole("heading", { name: `Today's work — ${TODAY_LABEL}` }),
  ).toBeVisible();
  await expect(
    page.getByText(`Filed in ${FILING_DATE}.md`, { exact: false }),
  ).toBeVisible();

  await openPaths(page);
  await page
    .getByRole("radio", { name: /Same day/ })
    .click();

  // The choice is persisted through the normal config write, not held in the
  // component — a reload has to keep it.
  await expect(app.callsTo("set_config")).resolves.toEqual([
    expect.objectContaining({
      config: expect.objectContaining({
        dates: { archive_mode: "same_day" },
      }),
    }),
  ]);
  await expect(
    page.getByText(`is filed in ${TODAY}.md`, { exact: false }),
  ).toBeVisible();

  // After: the dashboard names Monday's own file, and asks the backend for it.
  await page.getByRole("link", { name: "Dashboard" }).click();
  await expect(
    page.getByText(`Filed in ${TODAY}.md`, { exact: false }),
  ).toBeVisible();
  await expect(page.getByText("today's standup")).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Daily Standup — August 03, 2026" }),
  ).toBeVisible();

  const reads = await app.callsTo("read_standup_file");
  expect(reads).toContainEqual({ date: TODAY });
});

test("Compile now targets the announced file, not the calendar day", async ({
  page,
  app,
}) => {
  await app.start(makeScenario());

  await expect(
    page.getByText(`Filed in ${FILING_DATE}.md`, { exact: false }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Compile now" }).click();

  // The single assertion the reported bug comes down to: the button compiles
  // the file the header named.
  await expect(app.callsTo("preview_regeneration")).resolves.toEqual([
    { date: FILING_DATE },
  ]);
});

test("a manual item is added to the standup being written", async ({
  page,
  app,
}) => {
  await app.start(makeScenario());

  await page.getByRole("tab", { name: "Manual item" }).click();
  await page
    .getByRole("textbox", { name: "Note" })
    .fill("Paired on the release checklist");
  // The default target is "This standup", which is the filing date — not the
  // calendar day the label used to claim.
  await expect(
    page.getByText(`This standup — ${FILING_DATE}`),
  ).toBeVisible();
  await page.getByRole("button", { name: "Add", exact: true }).click();

  await expect(app.callsTo("add_manual_item")).resolves.toEqual([
    { date: FILING_DATE, item: "- Paired on the release checklist" },
  ]);
});

test("says nothing is filed yet when the target file does not exist", async ({
  page,
  app,
}) => {
  // Only yesterday's file exists — the normal state before the day's first
  // compile. The empty state must name the *target*, or the user goes looking
  // for a file the app never intended to write.
  const scenario = makeScenario();
  scenario.state.standups = { [TODAY]: makeStandupFile() };
  await app.start(scenario);

  await expect(
    page.getByRole("heading", { name: "No standup filed for Aug 4, 2026" }),
  ).toBeVisible();
  await expect(
    page.getByText(`Filed in ${FILING_DATE}.md`, { exact: false }),
  ).toBeVisible();
});

test("surfaces a failure to resolve the target instead of guessing a file", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  scenario.errors.get_filing_target = {
    code: "config",
    message: "config store is corrupt",
  };
  await app.start(scenario);

  await expect(
    page.getByRole("heading", { name: "Could not resolve today's standup file" }),
  ).toBeVisible();
  await expect(page.getByText("config store is corrupt")).toBeVisible();

  // And it must not have read or announced some other file in the meantime.
  await expect(app.callsTo("read_standup_file")).resolves.toEqual([]);
  await expect(
    page.getByText("Filed in", { exact: false }),
  ).toHaveCount(0);
});

test("keeps the weekend rule true for the same-day policy too", async ({
  page,
  app,
}) => {
  // Sunday 2026-08-09: no standup is named after a weekend day, so both
  // policies have to point at Monday's file.
  const scenario = makeScenario();
  scenario.state.config.dates = { archive_mode: "same_day" };
  scenario.state.standups = {
    [FILING_DATE]: makeFilingDateStandupFile(),
  };
  await app.start(scenario, "/", { now: "2026-08-09T12:00:00.000Z" });

  await expect(
    page.getByText("Filed in 2026-08-10.md", { exact: false }),
  ).toBeVisible();
});
